# FST propose+confirm performance gotchas

Restored from `2671bb41` (2026-08-14), audited against `ee5e4415` (2026-11-17). This catalogue
preserves July–August measurements and designs; it does not report a new corpus run. Language names
identify the published measurements only; no lexemes or corpus data are included. **MEASURED** means
a recorded run, **PROPOSED** means a design, and **HISTORICAL** means an older implementation or
observation. **VERIFIED** below is qualified by either that historical source or the current symbol
cited. Historical measurements do not establish current performance.

## Current runtime and evidence

The shipped backend is **Tuned surface probed**. #175 removed Templated underlying tokens and Plan
composed, including `build.rs`, `uflexc.rs`, and `templated_compile.rs`; see
[Removed FST backends](../fst-plan/removed-backends.md). Replace/gate research remains useful, but
its presence does not make a selectable composed backend.

For batch, #343 added `pangloss batch <grammar> <words> --engine fst`; `--engine hc` is the
default. The old `pangloss --engine=foma` spelling is historical. The FST path requires
`foma-tools`, `--threads 1`, `--work-cap unbounded`, `--word-mem-cap-mib 0`, and
`--capture-cap-mib unbounded`, and never silently falls back. It requires an external watchdog with
a deadline and memory ceiling; see `rust/crates/pg-cli/src/surface.rs`, `BATCH_FLAGS`. The current
compile/apply seam is `Session::compile` in `rust/crates/pg-cli/src/fst_engine.rs`, using
`pg_foma::analyzer::compile_proposer` and `FomaAnalyzer::from_cached_with_morpher` in
`rust/crates/pg-foma-runtime/src/composite.rs`. Confirmation is Rust HermitCrab, a port under test,
rather than a C# oracle replay.

**Recall is a correctness obligation, not a consequence of confirmation.** A permissive proposer can
add confirm work; an under-generating proposer can lose valid analyses that confirmation cannot
recover. Since #339, admission requires a conservative positive proof and refuses by default when
that proof is unavailable, including unsupported variable and structural rewrite forms. See
[the 0.22.1 changelog](../../CHANGELOG.md#fixed) and `refusals` in
`rust/crates/pg-foma/src/proposal_recall.rs`, sharing the emitter's
`eager_route_has_single_segment_spellings` and `pg_rules::rewrite::pattern_has_single_spellings`.
The backend scoreboard enforces zero compile-but-miss cells
(`rust/crates/pg-foma-backend/tests/backend_scoreboard_gate.rs`); this is still not a universal
equivalence proof. Keep capability
correctness, production readiness, and resource containment separate under the
[stress/admission contract](../superpowers/specs/2026-08-23-stress-grammar-construction-and-production-admission.md). A readiness Error does not justify skipping a contained stress attempt; raising a limit does not
make incomplete output complete.

For a current differential check, #334 shipped
[`pangloss fst-verify <grammar> <words>`](../formats/fst-verify.md). It compares FST plus
confirmation with plain **Rust** HC using ordered morpheme keys, root position, and category; the
verdict compares identity sets, while JSON retains identity multiplicities. It is not a C#
certificate. Inspect both missing and extra identities, row completion, and totals: an exit-zero run
may still contain incomparable rows. #346 added typed `capability_causes` and
`proposal_recall_occurrences` to construction attempts. Counts describe witnesses produced by the
owning check before deduplication, not rules, analyses, or words; `null` means unevaluated and `[]`
means evaluated with no witnesses. See the same format document for the exact scope.

Historical code lookup: `rust/crates/pg-foma/src/emit.rs` at `2671bb41` contains
`TextMode::UnderlyingTokens` and the dedicated-level chain; `rust/crates/pg-foma/src/build.rs`
at that revision contains `boundary_cleanup_net`, `finish_controllable_net`, and
`reroute_null_shaped_affix_chains`. `rust/crates/pg-foma/src/morphotactics.rs` at `eb4eed4`
contains `EnumerationBudget`; `rust/crates/pg-foma/src/compose_budget.rs` there contains the old
state/arc/group guards. `rust/crates/pg-foma/src/recipe_runtime.rs` at `eb4eed4` contains the
unbounded `evaluate_plans_marked` call. That module was renamed/replaced by `f49d12ae`, before
the later backend removal; do not search for it as a current API.

## Historical census baseline

The following baseline and numbered entries retain their original measurement context. Each
current-status note overrides older implementation/status wording in its entry. Designs described as
parked remain historical proposals, not an instruction to restore a removed backend.

**Historical observation underlying entries 1–5.** The `dead-end-census` skill's measured finding,
repeated on the grammars censused in July 2026: **91–98% of failing-candidate confirm time is
"cascade dead-ends"** — HC exhaustively proving no derivation exists — not shallow final-gate
rejections
([.claude/skills/dead-end-census/SKILL.md](../../.claude/skills/dead-end-census/SKILL.md#the-one-thing-to-understand-first);
[docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#why-this-exists-the-money)). A "magic sets"/pre-filter style screen sitting between propose and confirm was tried and killed
on measurement for exactly this reason — it can only intercept the 0–3% of cost sitting at the
shallow final gate, not the dominant mass, which dies deep inside confirm's own restricted reparse
([docs/fst-plan/grammar-optimization-techniques.md](../fst-plan/grammar-optimization-techniques.md#e-search-and-pruning),
citing the merged NO-GO at `571b8a3`). **The census motivated tightening what the FST proposes in
the first place** (reducing per-attempt confirm cost is a separate lever) — which is why every entry
below either names a proposer encoding that removes a class of junk candidate, or names a grammar
shape for which no such encoding yet exists.

The historical precision plan required tightening the candidate set (new ⊆ old) while preserving all
confirmable analyses. Only excess proposals cost speed alone; missing proposals can lose analyses
despite confirmation
([.claude/skills/dead-end-census/SKILL.md](../../.claude/skills/dead-end-census/SKILL.md#the-one-thing-to-understand-first)).

---

## Quick reference

| # | Dead-end class / gotcha | Evidence (paths from repository root unless linked) | One-line description |
|---|---|---|---|
| 1 | d1 — allomorph environment dead end | `rust/crates/pg-rules/src/stratum.rs`, `synthesize_stratum_traced_with_policy`; [docs/research/pg-foma-deadend-census.md](pg-foma-deadend-census.md#deadend_census--dead-end-attribution-design) | An allomorph's context restriction fails against an intermediate shape after the FST already committed to that allomorph — licenses composed boundary-marked context restrictions (E1) only when a grammar is genuinely environment-dense |
| 2 | d2 — disjunctive-block dead end | [docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#phase-0--dead-end-attribution-census-measure-before-building-gono-go-per-encoding) | First-match-wins allomorph block picks differently than the FST's segmentation assumed — licenses a priority-union (`.P.`) proposer encoding (E3) for grammars with large disjunctive blocks |
| 3 | d3 — feature/MPR unification clash | July plan linked in row 2 | Two pinned morphemes' feature structures don't unify — dominated in the reference corpus by the compounding head-gate, not affix agreement; real driver for a future agreement-heavy grammar |
| 4 | d4 — shape mismatch (no rule sequence reproduces the surface) | July plan linked in row 2; [docs/fst-plan/p6-prototype-report.md](../fst-plan/p6-prototype-report.md#p6-prototype-report--replace-rule-compilation-feasibility) | No composed rule ordering derives the surface string — the dominant class on phonology-heavy grammars; licenses replace-rule compilation (E2) |
| 5 | d5 — ordering/slot violation via shared template joins | July plan linked in row 2; `rust/crates/pg-rules/src/stratum.rs`, `synthesize_stratum_traced_with_policy` | The FST proposes a morpheme set/order no stratum+template ordering can realize, because sibling templates share a join point — the largest class on Sena and second-largest on Indonesian, and the one no pre-planned encoding covered |
| 6 | Blanket null-morph boundary-marker erasure | `rust/crates/pg-foma/src/build.rs` at `2671bb41`, `finish_controllable_net` / `boundary_cleanup_net` (fixed by `9cb569f`) | Composing away every occurrence of a semantically load-bearing null/zero-morph char-def the same way as an ordinary separator turns required transitions into free branches — up to 516× candidate explosion on the affected words |
| 7 | Fusion-class composite enumeration blow-up | [docs/fst-plan/morphotactic-composite-pruning.md](../fst-plan/morphotactic-composite-pruning.md#morphotactic-pruning-for-composite-pre-expansion-aweti-scale-fix) | A grammar with many optional, loose (non-template-slot) rules on an `Unordered` stratum against a large root set makes flat composite pre-expansion combinatorially explode; pruning bounds the *search*, not the *emitted network size* |
| 8 | Deep-chain multiplicative tag-choice blowup | [docs/fst-plan/p6-deep-truncation-chain-report.md](../fst-plan/p6-deep-truncation-chain-report.md#1-chain-restriction--shipped-verified-no-regression) | A shared derivation-chain automaton that offers every rule at every level lets one rule's tag be chosen redundantly at up to N levels, multiplying path count 22–48× along a single derivation |
| 9 | MPR `Overwrite` groups (non-monotone feature update) | `pg_grammar::model::mpr_add_output`; `rust/crates/pg-foma/src/capability.rs`, `MprGroupOverwritePredicate` | A later rule's feature-group output *replaces* rather than unions the accumulated state — historically treated as a refusal; now ConfirmOnly, enforced by HC |
| 10 | Flags used for adjacency, or inside a `->` replace rule | `rust/crates/pg-foma/src/gate.rs`, module documentation | Persistent flag diacritics cannot encode a left/right environment, and a flag literal inside a replace rule's own context corrupts or silently empties the composed network |
| 11 | Gated-subrule cross-product / checkpoint-free composition blowup | `rust/crates/pg-foma/src/gate.rs`; `rust/crates/pg-foma-runtime/src/compose_budget.rs` | Independent gated MPR/POS keys can produce many realized groups, and no vendored compose/minimize call can be checkpointed mid-operation — a blowup inside one call is invisible until it has already happened |

---

## 1. d1 — allomorph environment dead end

**Current-status note (2026-11-17):** Historical census and E1 proposal. No new environment-density
measurement is claimed here.

**Class definition (VERIFIED,
[docs/research/pg-foma-deadend-census.md](pg-foma-deadend-census.md#deadend_census--dead-end-attribution-design)):** a failing candidate's deepest traced frontier is an allomorph environment check that failed
against the intermediate shape the branch had reached. The census
(`rust/crates/pg-foma-backend/examples/lab/deadend_census.rs`) attributes this by walking a combined
analysis+synthesis trace tree per failing candidate and finding the frontier node — the failed
attempt with the greatest count of successful-(un)apply ancestors — then timing its class's
counterfactual share under the real batched `confirm_batch`
([docs/research/pg-foma-deadend-census.md](pg-foma-deadend-census.md#frontier-definition-the-thing-that-decides-the-numbers)).

**Mechanism.** The FST proposes a segmentation committing to a specific allomorph before confirm
ever checks whether that allomorph's declared left/right context actually holds at that position in
the derivation. If the FST's lexc network has no way to encode "this allomorph is only reachable
when its context restriction holds," every context-illegal segmentation the network's alphabet
otherwise permits becomes a proposed candidate that confirm must walk deep into a rule cascade to
reject — not a shallow check, because the environment can only be evaluated against an
*intermediate* derivational shape, not the bare input.

**Grammar shapes that trigger it, per the reference-corpus census (MEASURED,
[docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#phase-0-results-2026-07-17-merged-328b5e8-verdicts-are-the-main-loops)):** d1 measured ≤1.5% of failing-candidate time on Sena (which has 72 allomorph environment
constraints, so environment density alone does not predict this class — see also the falsified
predictor recorded in
[docs/research/README.md](README.md#what-has-been-tried-here-and-does-not-work)), ~4% (directional
only, denominator too small to trust) on Indonesian, and ~0% on Amharic. **Never crossed the
20%-of-failing-time go-bar on any of the three reference grammars**, so the July plan parked E1. It
stays a real risk for a grammar whose morphology is genuinely dense in first-match-sensitive,
position-dependent allomorph environments — something none of the three reference grammars happens
to exhibit despite Sena's raw environment *count*.

**Fix, if licensed (PROPOSED — E1, parked build-ready):** emitter v2 keeps a morpheme-boundary
symbol on the queryable lexc tape instead of stripping it, emits each allomorph's environment
constraint (already preserved in `ConstraintCatalog` from the torn-down runtime precision knob) as a
foma context-restriction regex over the boundary-marked tape, composes all restrictions with the
lexicon offline, then composes a final boundary-deletion transducer so the boundary symbol never
reaches the surface
([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#e1--boundary-marked-emit--composed-environment-restrictions-parked-targets-d1)).
Adjacency is native to composition, which is why this is the *only* legitimate mechanism for
environment constraints — see entry 10 for why flags cannot do this instead. Any constraint feature
not expressible as a segment-class context restriction declines permissively (stays proposed; only
costs speed).

**Go/no-go rule for a new grammar:** run `dead-end-census`; build E1 only if d1 crosses ≥20% of
failing-candidate time *and* the projected end-to-end win crosses ≥15% of that grammar's confirm
time
([.claude/skills/dead-end-census/SKILL.md](../../.claude/skills/dead-end-census/SKILL.md#4-apply-the-go-bar--per-grammar)).

---

## 2. d2 — disjunctive-allomorph block dead end

**Current-status note (2026-11-17):** Historical census and E3 proposal; first-match semantics must
be checked against the applicable HC rule family.

**Class definition (VERIFIED,
[docs/research/pg-foma-deadend-census.md](pg-foma-deadend-census.md#deadend_census--dead-end-attribution-design)):** the frontier is a "first-match-wins" disjunctive allomorph block that resolves to a *different*
allomorph than the one the FST's segmentation assumed for that candidate.

**Mechanism.** HermitCrab's disjunctive-allomorph semantics are first-match, not "any legal match":
when several allomorphs of a slot could apply, only the first one whose environment holds actually
fires; the others are dead by construction, no matter how plausible their own environment looks in
isolation. A proposing FST that offers every disjunctive alternative as an independent path (because
encoding "first-match-wins" as a language-level constraint is harder than encoding a plain union)
generates one candidate per alternative even though at most one is ever confirmable for a given
context — every other alternative is a d2 dead end.

**Measured
([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#phase-0-results-2026-07-17-merged-328b5e8-verdicts-are-the-main-loops)):**
d2 was summarized as ≤2.6% in the plan for the reliable reference measurements (Sena ~1.4-1.5%,
Amharic ~0%, Indonesian ~5% directional-only) — below the go-bar in those runs, despite Sena's own
`DisjunctiveAllomorph` construct count running 250–641 by raw count, which is exactly why the plan
explicitly warns "don't assume [small]" for the next grammar.

**Fix, if licensed (PROPOSED — E3, parked):** compile a first-match block as a priority union (`.P.`
in foma's regex calculus) over the block's alternatives with their contexts, composed per block, so
only the reachable first-match alternative survives in the network rather than every alternative
independently
([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#e3--disjunctive-block-priority-parked-targets-d2)).

---

## 3. d3 — feature-structure / MPR unification clash

**Current-status note (2026-11-17):** Historical census and E4 proposal. Current MPR handling is
ConfirmOnly (entry 9), so “no feature encoding” below describes this historical lexc comparison, not
every current admission check.

**Class definition (VERIFIED,
[docs/research/pg-foma-deadend-census.md](pg-foma-deadend-census.md#deadend_census--dead-end-attribution-design)):** the frontier is a feature-structure unification or MPR-feature clash between the pinned
morphemes a candidate's segmentation implies.

**Mechanism and the caveat that matters most here.** The FST's lexc network typically tracks little
or no feature-structure state along its continuation-class graph (feature agreement is exactly the
kind of long-distance, non-adjacent constraint flags were designed for — see entry 10 — but no
feature encoding is built yet). So the FST proposes morpheme combinations whose feature structures
would never unify, and confirm walks the full unification machinery before discovering the clash.

**Measured, and the surprising attribution
([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#phase-0-results-2026-07-17-merged-328b5e8-verdicts-are-the-main-loops)):**
d3 crossed 14–17% on Sena and 14.4% on Amharic — close to, but under, the 20% go-bar — with one
sharp qualifier: on Sena, **4,966 of 5,090** d3 candidates trace to
`HeadRequiredSyntacticFeatureStruct` specifically — the *compounding* head-gate, not ordinary affix
agreement. That means d3's mass here is root+root compound over-generation (the compounding
permissive-admission gate deliberately stays wide to protect recall of legitimate compounds), a
different and likely cheaper problem than a general feature-bundle encoding would solve. The plan's
own instruction: split d3 by analysis/synthesis side and rule kind before ever building a feature
encoding for it (the cited July plan).

**Fix, if licensed (PROPOSED — E4, parked, "borderline… with a caveat that may redirect it"):**
partition continuation classes by a coarse, census-derived finite feature signature (only the
specific features shown to actually clash) — or, for long-distance agreement families genuinely
suited to it, U/R/D-typed flags
([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#e4--coarse-feature-bundle-compatibility-parked-targets-d3)).
This is the highest-blowup-risk parked encoding of the four (exponential in interacting features) —
build it last, budget hardest, and only after confirming which side of the analysis/synthesis
boundary the mass is really on. **A future agreement-heavy grammar (concord systems, broken-plural
paradigms) is the plan's own named trigger for promoting this out of parked status.**

---

## 4. d4 — shape mismatch (no rule sequence reproduces the surface)

**Current-status note (2026-11-17):** HISTORICAL E2 prototype. The selectable templated/composed
backends were removed in #175; current Tuned surface probing is subject to #339 proposal-recall
refusal checks. The old hybrid/prototype description below is not the current production pipeline.

**Class definition (VERIFIED,
[docs/research/pg-foma-deadend-census.md](pg-foma-deadend-census.md#deadend_census--dead-end-attribution-design)):** the frontier is a phonological shape mismatch — no sequence of rule (un)applications the
confirm cascade tries reproduces the candidate's surface string.

**Mechanism.** PanGloss's mainline emitter bakes phonology into literal enumerated strings at build
time rather than compiling phonological rewrite rules into the FST as rewrite calculus ([Removed FST
backends](../fst-plan/removed-backends.md), historical pipeline distinction). Where a grammar has
real phonological alternation, the FST proposes root+affix combinations whose *literal* stored
surface variants don't actually correspond to any legal rule-application sequence for that specific
combination — confirm has to run the whole rewrite cascade to find that out, because the check is
inherently sequential (rule 1's output shape gates whether rule 2 can apply, and so on).

**Measured
([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#phase-0-results-2026-07-17-merged-328b5e8-verdicts-are-the-main-loops)):**
d4 is **22–23% on Sena, 16.8% on Indonesian, and 28.6% on Amharic — the largest single class on
Amharic**, matching the plan's own prediction that this class dominates phonology-heavy grammars.
The plan licensed E2, but its table gives 16.8% for Indonesian, below the stated 20% class
threshold. Preserve that discrepancy rather than claim all grammars crossed it.

**Historical E2 prototype mechanism ([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#e2--replace-rule-compilation-for-phonology-build-after-e5-targets-d4-amharic); prerequisite feasibility from
[docs/fst-plan/p6-prototype-report.md](../fst-plan/p6-prototype-report.md#p6-prototype-report--replace-rule-compilation-feasibility)):** emit UNDERLYING forms instead of pre-resolved surface strings; compile each phonological
subrule to a foma `->` replace rule (feature contexts lowered to segment classes; α-variables
tuple-expanded, bounded by the grammar's actual feature domains); compose the cascade in stratum
order; project the surface side. This retires the pre-expansion enumeration bridge for every rule it
can compile; rules it cannot compile stay pre-expanded (a hybrid emit that is upward-safe by
construction — it can only ever decline to tighten, never lose recall). **A grammar with a rich,
genuinely productive phonological rewrite cascade (vowel harmony, extensive assimilation/epenthesis
chains) is the shape most likely to need this before anything else in this document.**

---

## 5. d5 — ordering/slot violation via shared template joins

**Current-status note (2026-11-17):** HISTORICAL E5 design and census. Current emit still has shared
joins, but contains no TMPL flag binding. The July plan is not proof that its flag design shipped in
the remaining backend. Current `synthesize_stratum_traced_with_policy` rejects remaining stratum
rules after its non-final-template check; PartialParse is not a universal proof that every other
final condition passed.

**Class definition (VERIFIED,
[docs/research/pg-foma-deadend-census.md](pg-foma-deadend-census.md#deadend_census--dead-end-attribution-design)):** the frontier is a stratum-order or affix-template slot-order violation — the FST proposed a
rule sequence the engine's own morphotactic walk can never actually realize.

**Why this class is the census's headline finding.** E1–E4 above were all planned *in advance* of
the first attribution run; none of them targets ordering. The 2026-07-17 census found d5 dominating
two of three reference grammars anyway — **the single largest class measured on Sena (49.5–56.7%
across sample sizes) and the second-largest on Indonesian (20.3%)**
([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#phase-0-results-2026-07-17-merged-328b5e8-verdicts-are-the-main-loops)).
The plan states this plainly as the reason never to pre-commit an encoding roster ahead of
attribution (the cited July plan).

**Mechanism, localized by direct sampling of real d5 frontiers
([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#e5--order-faithful-continuation-classes-targets-d5--build-first-new-this-census)).**
`PartialParse` was **100.00%** of the sampled d5 frontiers — zero `MaxApplicationCount`, zero
`NonPartialRule*AfterTemplate`. That is an attribution result for those runs, not a general claim
that all final checks passed. Current synthesis first rejects a non-final last template, then
reports remaining stratum rules as `PartialParse` (`rust/crates/pg-rules/src/stratum.rs`,
`synthesize_stratum_traced_with_policy`). The concrete mechanism differs by grammar shape:

- **Sena — template-slot order carries the mass, via shared group joins.** Templates that share an
  optional slot (the historical `G{gi}Join` construction in `rust/crates/pg-foma/src/emit.rs`
  at `2671bb41`; the join name remains in current emit) let one
  template's prefix chain link up with a *different, sibling* template's suffix chain through that
  shared join point, or let an optional slot reachable via a sibling template's shorter chain leave
  the home template's own mandatory slots unfilled. Worked example: a word segmentable as bare root
  plus one optional intensifier morpheme, with both of the home template's mandatory slots empty,
  where that intensifier slot is shared by ten different templates in the group — the FST proposes
  the combination because the shared join makes it a legal path through the network, but no single
  template's own morphotactic walk ever produces that combination. Stratum order itself contributes
  nothing to Sena's d5 mass; it is entirely a template-slot-order effect.
- **Indonesian — a different mechanism entirely, not touched by the same fix.** Indonesian has zero
  `<AffixTemplate>` elements (a single `unordered` stratum), so every cascade output finalizes
  automatically and `PartialParse` cannot come from template-slot sharing. Its d5 mass is
  reduplication-peeler candidate multiplicity instead (two peel variants offered against the same
  peeled shape, cross-producted) — a peel-precision fix, not an ordering encoding, would be the right
  lever there if ever prioritized.

**Historical E5 design, licensed first ([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#e5--order-faithful-continuation-classes-targets-d5--build-first-new-this-census)):** make the emitted lexc's continuation-class graph faithful to stratum order *and* affix-template
slot order, using **template-identity flag binding** across the prefix→suffix span rather than
un-sharing each template's own copy of the shared join (un-sharing would replicate the whole root
set per group member — Sena's own groups average 2.7× replication and the plan explicitly flags this
as "unbounded on future grammars"). Concretely: one overwrite-type flag family, `@P.TMPL.<v>@` set
on every exit from template `ti`'s prefix chain and `@R.TMPL.<v>@` at `ti`'s suffix entry inside
the shared join, with every group-entry point setting exactly one value. Template identity across a
prefix→suffix span is exactly the kind of *long-distance* agreement family flags remain legitimate
for — the teardown that killed flags elsewhere (entry 10) was specifically about *adjacency*, which
template identity is not. Un-sharing is kept on record as the fallback if flags trip the propose-p95
budget or hit a demonstrable foma-rs defect.

**Grammar shapes most likely to need this:** any grammar whose affix templates share optional slots
across a template group (the Sena mechanism), or whose stratum has heavy peeler-based candidate
multiplicity for reduplication/copying constructs (the Indonesian mechanism, unfixed by E5 and
needing its own peel-precision pass).

---

## 6. Blanket null-morph boundary-marker erasure (HISTORICAL — fixed, mechanism still instructive)

**Current-status note (2026-11-17):** HISTORICAL fixed prototype defect. #175 deleted the affected
build module; the representation lesson remains relevant. The original proposed boundary-deletion
alternatives below are not current production instructions.

**Historical status: fixed 2026-07-30 by `build::reroute_null_shaped_affix_chains` at `2671bb41` (deleted by
PR #175) (`9cb569f`)
([docs/fst-plan/large-lexicon-proposal-explosion.md](../fst-plan/large-lexicon-proposal-explosion.md#large-lexicon-proposal-explosion-diagnosis-sena-2026-07-29));
kept here because the pathology recurs in shape for any grammar whose char-def table declares a
semantically load-bearing `Boundary` kind.**

**Mechanism, as measured against the affected build path (`rust/crates/pg-foma/src/build.rs` at
`2671bb41`, `finish_controllable_net` / `boundary_cleanup_net`).** A grammar's char-def table can declare more than one `Boundary`-kind
char-def, and not all of them mean the same thing: an ordinary morph-boundary separator (e.g. `+`)
versus a **null/zero-morph marker family** — a token whose presence signals "a zero-realized morph
occurred here," which is grammatically load-bearing information, not cosmetic punctuation (Sena's
own char-def table declared exactly this three-way split:
[docs/fst-plan/large-lexicon-proposal-explosion.md](../fst-plan/large-lexicon-proposal-explosion.md#the-mechanism)). One code path composed a single context-free regex that deleted **every** `Boundary`-kind
char-def identically, with no reference to which rule instance licensed a given occurrence. For an
ordinary separator this is harmless; for a null-morph marker it erases the exact adjacency
information that used to make a continuation-class choice *unique* at that state — after blanket
deletion, the automaton can non-deterministically choose any of the formerly boundary-distinguished
continuation classes at that state, and confirm then dead-ends on nearly all of them.

**Measured impact
([docs/fst-plan/large-lexicon-proposal-explosion.md](../fst-plan/large-lexicon-proposal-explosion.md#direct-ab-experiment-not-assumed--measured)).**
On the affected build path, the same grammar and word set produced 127 candidates through the
(unaffected) production emitter versus 53,992 through the affected path — a 425× blow-up
concentrated overwhelmingly (99.5%) on words whose morphology crosses a null-morph-marker boundary
at a productive slot (nasal-class prefixation-shaped words in the diagnosed grammar); words with no
such juncture showed no blow-up at all. Every one of eight independently-pinned worst-case words
exceeded a 15-second cutoff under the affected path while completing sub-2.2s under the production
path.

**Fix that shipped, and the general lesson.** The general principle this incident establishes for
any grammar with a similar char-def shape: **never place a semantically distinguishing
boundary/null-morph token on the queryable surface tape and hope a later, blanket compose-time
deletion is safe.** The production emitter's own convention — enumerate representation variants at
emit time, drop boundary characters before they ever reach the network — is the correct pattern; a
context-restricted, per-occurrence deletion (only stripping a boundary token within the specific
environment the rule that inserted it actually licenses) is the documented fallback where the
enumerate-at-emit-time pattern isn't available. **A grammar with a distinct null/zero-morph char-def
kind is the shape to watch for**, and the fix confirms this is a construction-correctness issue, not
a dead-end-census tuning question — none of the d1–d6 classes or their E1–E5 encodings apply to it.

---

## 7. Fusion-class composite enumeration blow-up (large loose-rule / Unordered-stratum lexicons)

**Current-status note (2026-11-17):** Historical measurements and budget defaults. Current pruning
lives in `rust/crates/pg-foma-runtime/src/morphotactics.rs` (`MorphotacticIndex`); flat recursion
below describes the pre-pruning builders. `EnumerationBudget` is no longer a live type. Current
closure/structural work and byte controls replace those historical caps; see the fst-verify format
for their CLI surface. No current grammar buildability is inferred from the old corpus run.

**Historical status: PARTIAL — the search-time fix landed; the output-size problem remained open in
that report
([docs/fst-plan/morphotactic-composite-pruning.md](../fst-plan/morphotactic-composite-pruning.md#morphotactic-pruning-for-composite-pre-expansion-aweti-scale-fix)).**

**Mechanism.** Two composite-chain builders (`preexpand::extend` for
interdigitation/boundary-fusion, `emit::struct_extend` for truncation/circumfix/probe-refusal
composites) recursively chain every candidate rule onto every root at every depth, gated only by a
cheap feature-structure unifiability pre-filter, and — critically — neither one consults the
grammar's affix templates or stratum rule order at all
([docs/fst-plan/morphotactic-composite-pruning.md](../fst-plan/morphotactic-composite-pruning.md#problem)).
A grammar shaped like "many roots, many candidate rules, most of them loose (legal in any order)
rather than template-slot-bound, on an `Unordered` stratum, with a substantial fraction of
fusion-class rules and few or no infix rules" (the measured Aweti shape: 855 roots × 123 candidate
rules, 47 loose rules, 88 of 135 total mrules slot-only) explores rule orderings the engine's own
synthesis morphotactics can *never* actually produce — the flat recursion is combinatorially wasted
work, not genuine ambiguity.

**Measured
([docs/fst-plan/morphotactic-composite-pruning.md](../fst-plan/morphotactic-composite-pruning.md#amharic-ab-numbers-the-sizing-tables-real-tree-confirmation),
the sizing table's real-tree confirmation).** On a smaller reference grammar (76 roots × 87 rules),
morphotactic-pruning (which restricts the recursion to only the rule adjacencies the engine's own
stratum/template/slot walk actually permits) shrank the search from 305,621 to 104,605 probed pairs
— a 2.92× reduction — and wall time from 7.34s to 1.06s, with the pruned entry set formally verified
a subset of the flat one (zero pruned entries missing from flat). Pruning restricts the flat
recursion using an over-approximation of legal morphotactics. Subset containment alone does not
prove recall; the morphotactic argument and recall gates are also needed.

**But pruning bounds only the search, not the output, and that is the sharper gotcha
([docs/fst-plan/morphotactic-composite-pruning.md](../fst-plan/morphotactic-composite-pruning.md#aweti-end-to-end-result-2026-07-18-pruning-is-necessary-but-not-sufficient)).**
On the Aweti-shaped grammar, pruning made the previously-OOMing build *complete* (bounded, ~551s),
but the resulting network was still **~124× a working reference grammar's fusion-entry count**
(2,833,559 fusion entries versus 22,775), producing a 691MB / 9.7-million-line lexc source that
foma's own compiler took ~223s to consume, and whose `apply_up` then failed outright on the very
first corpus word with an ~8.8GB allocation. **The emitted composite set for a
fusion-class-dominant, largely-loose-rule, Unordered-stratum grammar can be too large for propose to
consume at all, however cheaply it was built** — enumeration (pruned or not) is not a viable
end-to-end strategy for this grammar shape.

**Historical defensive fix and proposed replacement.** The then-default fail-fast
`EnumerationBudget` (`rust/crates/pg-foma-runtime/src/morphotactics.rs` (pruning); historical
`EnumerationBudget`) tracks cumulative composite-entry count (primary, disaster-predicting measure
— default cap 200,000) and (root, rule) pairs probed (secondary backstop — default cap 3,000,000)
across both composite builders, latching a shared flag checked at the top of every recursive call so
a doomed build aborts in seconds rather than after ~551s
([docs/fst-plan/morphotactic-composite-pruning.md](../fst-plan/morphotactic-composite-pruning.md#addendum-fix-1--default-on-fail-fast-enumeration-budget-2026-07-20)).
This turns a silent 13-minute crash into an immediate, typed refusal — it does not make the grammar
buildable. The report proposed replace-rule compilation (P6/E2, entry 4 above) generalized to
templated morphotactics, "independently shown to compile [phonological rules] into a tiny network
(30 states/2143 arcs, 28.8ms) with no enumeration blow-up at all" for the rule layer alone — not yet
wired up to a fully templated morphotactic layer
([docs/fst-plan/morphotactic-composite-pruning.md](../fst-plan/morphotactic-composite-pruning.md#aweti-end-to-end-result-2026-07-18-pruning-is-necessary-but-not-sufficient)).
A cheap, cheaper-than-a-full-emit predictor for this grammar shape is explicitly still missing:
`composite_scale_hint` (should-run / candidate-rule-count / root-count) did **not** predict the
Aweti explosion — the grammar looked ordinary on all three signals before the 9-minute emit revealed
the problem.

---

## 8. Deep-chain multiplicative tag-choice blowup

**Current-status note (2026-11-17):** HISTORICAL underlying-token chain and recipe-runtime incident.
PR #175 removed the relevant backend; `recipe_runtime` is absent from the current tree; current
`emit::build_deriv_chain` still offers every rule at every level. The dedicated-level fix applied
only to the removed underlying-token mode. Current optimizer evidence carries oracle
work/memory/liveness configuration (`OracleEligibilityConfig` in
`rust/crates/pg-foma-backend/src/backend_optimizer.rs`). The unbounded call below is an old
incident, not an outstanding claim about that removed call site. `--step-cap` was replaced by
`--work-cap` (#133).

**Historical status: shipped search-space fix; a separate unbounded-oracle-call hazard was recorded
in the tooling of that period
([docs/fst-plan/p6-deep-truncation-chain-report.md](../fst-plan/p6-deep-truncation-chain-report.md#1-chain-restriction--shipped-verified-no-regression);
[docs/fst-plan/deep-chain-pilot-non-completion.md](../fst-plan/deep-chain-pilot-non-completion.md#why-the-deep-truncation-chain-grammars-recipe-optimize-pilot-never-completes)).**

**Mechanism.** `build_deriv_chain`'s original strategy builds a shared derivation-chain automaton
where every level offers every rule in a zone's rule set (`rules.len()` levels). For a grammar with
many standalone rules that each yield zero or minimal surface material (epsilon-like), a single
rule's tag becomes choosable at *any* of those levels independently — for a measured 11-rule prefix
/ 24-rule suffix zone wired through two independent chain instances, one epsilon-yielding rule's tag
was choosable up to **22× (prefix) / 48× (suffix)** along a single path
([docs/fst-plan/p6-deep-truncation-chain-report.md](../fst-plan/p6-deep-truncation-chain-report.md#1-chain-restriction--shipped-verified-no-regression)).
This directly produces `PATHCOUNT_OVERFLOW` -scale finite ambiguity and effective non-termination of
`apply_up` on some queries — not because the grammar has genuine unbounded ambiguity, but because
the chain representation multiplies out redundant tag placements that all describe the same
derivation.

**Measured
([docs/fst-plan/p6-deep-truncation-chain-report.md](../fst-plan/p6-deep-truncation-chain-report.md#1-chain-restriction--shipped-verified-no-regression)).**
Restricting each rule to its own dedicated level(s) (`rule.max_apps()` consecutive levels, capped
defensively at 4) shrank the composed net from 35,846 states / 800,354 arcs to 14,806 states /
270,541 arcs, with compose-based recall byte-identical before and after (zero regression), and
turned an `apply_up` query that previously would not complete even 500 raw results in 45 seconds
into one completing 2,000,000 raw results in ~2.1 seconds.

**A related, separate hazard the fix does not touch: unbounded oracle calls hidden in tooling.**
Independent of the FST-side fix, the recipe-optimizer's ground-truth oracle call
(`rust/crates/pg-foma/src/recipe_runtime.rs` at `eb4eed4`, `evaluate_plans_marked` (removed
module)) constructs `pg_parse::Morpher::new(grammar, usize::MAX)` — an uncapped step budget with no
wall-clock timeout chained on either — and this call simply does not return for at least one word in
a deep-chain grammar's own corpus (measured: >20s with no completion, versus 91.6ms with a `20_000`
step cap or 2.8s with a 2-second wall-clock timeout on the identical call,
[docs/fst-plan/deep-chain-pilot-non-completion.md](../fst-plan/deep-chain-pilot-non-completion.md#fresh-reproduction-this-build-this-checkout-head-eb4eed4)). Neither `ComposeBudget` nor `EnumerationBudget` is even reached in this failure mode — the hang
happens *before* any FST construction begins
([docs/fst-plan/deep-chain-pilot-non-completion.md](../fst-plan/deep-chain-pilot-non-completion.md#2-does-composebudgetenumerationbudget-trip-no--they-are-never-reached)).
**The grammar shape that triggers this is the same one that triggers the network-size problem:
many-level derivation chains with epsilon-yielding rules** — but the fix here is orthogonal to the
FST encoding; it is about threading a finite step cap or word timeout into any full-oracle call a
diagnostic or optimizer tool makes against such a grammar, since the existing
`Morpher::with_word_timeout` /step-cap machinery already used by `pangloss batch --word-timeout-ms`
and the historical `--step-cap` (replaced by `--work-cap` in #133) was never wired into that call site.

**Grammar shapes to watch for:** deep templated derivation chains (many standalone/floating-affix
rules chained across many "levels" in the `TextMode::UnderlyingTokens` emitter path), especially
where several of those rules are epsilon-realized or near-epsilon in their surface contribution.

---

## 9. MPR `Overwrite` groups — non-monotone feature-set update

**Current-status note (2026-11-17):** Current `MprGroupOverwritePredicate` returns ConfirmOnly when
Overwrite is observed and Admit when absent. This is not a permanent whole-grammar refusal. HC
enforces the order-dependent semantics; a future narrowing filter needs its own soundness proof. The
reachability and dual-rail constructions below are HISTORICAL research, not an implemented admission
requirement.

**Historical status (superseded by ConfirmOnly): capability carve-out by design
(`rust/crates/pg-foma/src/capability.rs`, `MprGroupOverwritePredicate`, id
`mpr-group.overwrite-output`), narrowable per-grammar by a reachability proof
([docs/fst-plan/mpr-overwrite-encoding-research.md](../fst-plan/mpr-overwrite-encoding-research.md#can-mprgroupoverwrite-be-compiled-under---enginefoma-four-constructions-evaluated)).**

**Mechanism.** `pg_grammar::model::mpr_add_output` (defined in
`rust/crates/pg-grammar-model/src/model.rs`, re-exported by `pg-grammar`) implements HermitCrab's
`MprFeatureSet.AddOutput` semantics precisely: for every `Overwrite`-policy feature group a rule's
output *touches*, every member of that group **not** restated by the new output is dropped from the
word's accumulated feature state before the new output is unioned in
([docs/fst-plan/mpr-overwrite-encoding-research.md](../fst-plan/mpr-overwrite-encoding-research.md#1-semantics-restated-precisely)).
This means the word's true state for that group at any point is exactly the *most recent touch's*
asserted subset — a genuinely order-dependent computation, not a monotone accumulation. A proposing
FST that tracks feature state as a plain accumulating union (the cheap, otherwise-safe approximation
used everywhere else) computes an over-approximation of the accumulated state when conflicting
touches are reachable. If used as a narrowing filter, stale retained members can exclude valid
analyses. Current code avoids that narrowing and leaves enforcement to HC; the old research refusal
is superseded.

**The historical reachability analysis of the reference grammars (MEASURED,
[docs/fst-plan/mpr-overwrite-encoding-research.md](../fst-plan/mpr-overwrite-encoding-research.md#2-the-reference-grammars-actual-shape-measured-not-assumed)).** A reachability predicate — "drop-unreachable": for every ordered pair of touch points `(P, Q)`
to the same group where `P` can feed `Q`, does `P`'s asserted subset stay a subset of `Q`'s? —
reduces to the already-safe `Append` baseline whenever it holds, at zero new FST cost (a
characterizer-side check reusing the same feeds-relation machinery already built for
compounding-depth analysis). Checked directly against the grammar sources, this predicate passes for
**every** `Overwrite` group in every reference grammar in that study — five of six groups because
nothing in the grammar ever actually touches them, the sixth because it is a singleton group (for
which `Overwrite` is algebraically identical to `Append`, Construction 1 in the linked study). **The historical
proposed carve-out targets a grammar that uses a multi-member `Overwrite` group with
reachably-conflicting touches** — a shape none of the reference grammars exhibits, but plausible for
a grammar with rich cross-cutting non-monotone morphosyntactic feature resetting.

**Historical alternative if a narrowing filter needs that shape and the reachability proof fails:**
the research identified a dual-rail/bilattice encoding tracking `(asserted, denied)` per group,
admitting a contradictory member as "impose no constraint" (safe, over-generating direction) — at a
genuine, multiplicative `O(4^k)` per-position state cost for a group of size `k`, threaded through
the rest of the derivation from the first touch onward
([docs/fst-plan/mpr-overwrite-encoding-research.md](../fst-plan/mpr-overwrite-encoding-research.md#construction-3--dual-rail--bilattice-admit-both-on-contradiction-the-general-fallback)).
**Flag diacritics are closed for this construct specifically** (see entry 10) — not merely
undischarged but empirically demonstrated unsafe at the exact site (`->` replace rules) where a
grammar's real MPR usage sits (Construction 4 in the linked Overwrite study).

---

## 10. Flags for adjacency, or flags inside a `->` replace rule

**Current-status note (2026-11-17):** The matched-context flag limitation remains documented in
current `gate.rs`. Its module documentation now distinguishes matched-context flags from a narrowly
tested inverted inserted-flag relation (`rust/crates/pg-foma-backend/tests/flag_replace_scope.rs`). Do not
generalize the old result to every use of flags with replace rules; the deleted template backend is
not an active flag consumer.

**Status: recorded dead end for matched-context flags in this specific composition context,
documented in `rust/crates/pg-foma/src/gate.rs`, module documentation; flags remain legitimate in
other proven contexts.**

**Mechanism.** Flag diacritics (`@P.X.Y@`/`@R.X.Y@`/etc.) set or test named registers along a
transducer path and are the classical tool for encoding long-distance agreement without blowing up
automaton size. They are structurally the *wrong* tool for a left/right **environment** constraint,
because an environment is an adjacency constraint and a persistent flag has no notion of position —
this was proven empirically, not just argued, by an under-generation bug on a recorded grammar case
and a 1.5GB micro-lexicon blowup when flags were tried for exactly this purpose
([.claude/skills/dead-end-census/SKILL.md](../../.claude/skills/dead-end-census/SKILL.md#hard-won-lessons-why-the-steps-above-are-shaped-this-way)).
Separately, and independently, three toolkit-level defects were found when flags were prototyped for
MPR/POS subrule gating specifically *inside* a `->` replace rule's own context
(`rust/crates/pg-foma/src/gate.rs`, module documentation, verified by two independent probe sessions
in
[docs/fst-plan/mpr-overwrite-encoding-research.md](../fst-plan/mpr-overwrite-encoding-research.md#construction-4--flag-diacritics-already-tried-in-this-exact-codebase-for-this-exact-purpose--and-abandoned)): (1) a flag literal inside a replace rule's own context compiles but returns a nondeterministic
mix of "rule fired"/"didn't fire" for identical repeated queries, and a context of *only* a flag
literal crashes the vendored minimizer outright; (2) `fsm_compose` does not treat flag symbols as
epsilon-transparent by default, so composing a flag-bearing network with a flag-free one can
silently collapse to the empty language even though the flag's own semantics (obeyed standalone)
would have passed; (3) a Kleene-star flag-gated workaround for (1) was itself order-fragile once
composed with a real lexc network.

**Grammar shapes to watch for.** Any construct that would naturally reach for flags to encode a
left/right context restriction (use composition over boundary-marked strings instead — entry 1's E1)
or to gate an MPR/POS feature *inside a phonological rewrite rule* specifically (the exact shape a
real grammar's `Overwrite` -group usage sits at, entry 9) — flags remain the right tool for
genuinely long-distance, non-adjacency families such as the historical template-identity binding
design across a prefix→suffix span (entry 5's E5) or feature agreement (entry 3's E4), where the
constraint being encoded is not positional.

---

## 11. Gated-subrule cross-product blowup, and the checkpoint-free compose call

**Current-status note (2026-11-17):** Historical prototype partitioning and calibration. Current
`gate.rs` retains the static partition design, but the composed backend is gone. Current
`ComposeBudget` in `rust/crates/pg-foma-runtime/src/compose_budget.rs` has chain-depth control; the
state/arc/group defaults described below are historical, not current production limits. External
process containment remains necessary for opaque native operations; a between-call check cannot
interrupt one.

**Historical status: partitioning used at small scale, formalization PROPOSED; checkpoint-free
compose — VERIFIED vendored-toolkit limitation
([docs/fst-plan/phase-b-compose-budget-design.md](../fst-plan/phase-b-compose-budget-design.md#1-vendored-crate-findings-foma--040-cratesio--cannot-be-patched-in-tree)).**

**Mechanism, part one: gate-group cross product.** `crate::gate`'s lexical partition-by-gating-key
groups lexical entries by their joint gated MPR/POS truth-value keys (`partition_entries` in
`rust/crates/pg-foma/src/gate.rs`, recorded as `GatePartitionSpec::groups` in
`rust/crates/pg-foma/src/plan.rs`), compiling each group independently and unioning the result
([docs/fst-plan/grammar-optimization-techniques.md](../fst-plan/grammar-optimization-techniques.md#c-factoring-and-decomposition--most-relevant-to-reallocate-into-new-construction-steps),
entry C4). The historical catalogue compares this joint partition to a one-shot elimination step. It was small in those
measurements (one gated subrule for one reference grammar, three for another) — but it uses *joint
keys* rather than eliminating the most-constraining (lowest-cardinality) feature first, which is
precisely the mechanism that keeps intermediate bucket sizes small once *several independent* gated
features interact. A grammar with more than a handful of independent gated MPR/POS subrules can have
up to `min(entry_count, 2^gated_subrule_count)` realized groups. It does not materialize empty
Cartesian buckets. Variable elimination was proposed as a way to reduce intermediate construction
cost; a smaller final partition is not established by that proposal
([docs/fst-plan/grammar-optimization-techniques.md](../fst-plan/grammar-optimization-techniques.md#c-factoring-and-decomposition--most-relevant-to-reallocate-into-new-construction-steps)).

**Mechanism, part two: no vendored operation can be checkpointed mid-call.** Every composition-heavy
encoding above (E1, E2, E4, and gate's own group compose) pays a cost inside `fsm_compose` /
`fsm_union` / `fsm_minimize`, and the vendored `foma` crate exposes **no mid-operation hook of any
kind** for any of them — confirmed by reading the function bodies directly, not inferred
([docs/fst-plan/phase-b-compose-budget-design.md](../fst-plan/phase-b-compose-budget-design.md#1-vendored-crate-findings-foma--040-cratesio--cannot-be-patched-in-tree)).
Worse, `fsm_compose` **internally minimizes both of its operands before composing**, so every single
compose step already pays a determinize — an operation whose worst case is exponential — not only
the final, explicit minimize a caller might expect to budget for
([docs/fst-plan/phase-b-compose-budget-design.md](../fst-plan/phase-b-compose-budget-design.md#1-vendored-crate-findings-foma--040-cratesio--cannot-be-patched-in-tree)).
A grammar whose composed encoding blows up does so *inside* one opaque library call; a between-call
size check (`ComposeBudget`'s `state_cap` /`arc_cap`, checked after each call returns) cannot detect
or stop a blowup that happens *during* the call that would have tripped it — it can only bound
accumulation *across* calls
([docs/fst-plan/phase-b-compose-budget-design.md](../fst-plan/phase-b-compose-budget-design.md#7-limitations-verbatim-from-the-investigation--the-honest-part)).
`fsm_union` specifically does not minimize per step, so a per-group union fold (as `gate.rs` does)
can silently accumulate a large non-minimal net whose eventual forced minimize — not any
intermediate step — is the true worst-case moment
([docs/fst-plan/phase-b-compose-budget-design.md](../fst-plan/phase-b-compose-budget-design.md#1-vendored-crate-findings-foma--040-cratesio--cannot-be-patched-in-tree)).

**Historical calibration (superseded; see current-status note).** The historical `ComposeBudget`
size caps defaulted to 2,000,000 states / 20,000,000 arcs — roughly 50×/25× the largest measured
real composed network at the time of calibration — specifically because the enumeration path's own
failure mode (an ~8.8GB single allocation, entry 7) is categorically worse than a composition path
failing a size check early
([docs/fst-plan/phase-b-compose-budget-design.md](../fst-plan/phase-b-compose-budget-design.md#8-review-addendum-2026-07-20-post-dfb5025)).
`gate.rs`'s own group-count budget (`DEFAULT_GROUP_BUDGET = 64` in the historical
`compose_budget.rs`) existed for exactly the cross-product risk in part one, tripping with **no
graceful fallback by design**: merging or dropping gated groups is unsound (it would over- or
under-fire gated rules), so the only correct response to a trip is a typed error refusing that
construction (current batch FST selection does not fall back)
([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#budgets-and-kill-switches-every-encoding-every-grammar),
budgets/kill-switches section).

**Grammar shapes to watch for:** several *independent* gated MPR/POS features interacting at once
(part one), and any composition-heavy encoding (E1/E2/E4, or a heavily gated grammar) approaching
its size budget — because the budget is a late-detection safety net, not a guarantee the blowup
itself is bounded in wall time or peak memory during the one call that produces it.

---

## Cross-cutting lessons from the historical runs

- **Run `dead-end-census` before hypothesizing.** Every one of entries 1–5's go/no-go verdicts is
  per-grammar, not a permanent ranking — a class that earns nothing on the reference corpus may
  dominate the next grammar, exactly as d5 (entry 5) dominated two of three grammars despite no
  encoding having been planned for it in advance
  ([.claude/skills/dead-end-census/SKILL.md](../../.claude/skills/dead-end-census/SKILL.md#4-apply-the-go-bar--per-grammar)).
- **Two falsified predictors, worth knowing before guessing.** Phonological-rule density does not
  predict which grammar is slowest — the slowest reference grammar in this corpus has zero rewrite
  rules ([docs/research/README.md](README.md#what-has-been-tried-here-and-does-not-work), citing `per-language-fst-synthesis.md` and
  `grammar-feature-space.md` §3.4). Raw probe count does not predict enumeration blow-up either — the
  actual predictor is emitted-entry count, not probe count ([docs/research/README.md](README.md#what-has-been-tried-here-and-does-not-work), citing
  `handspun-technique-audit.md` §2.15). Don't reach for either as a quick diagnostic in place of the
  census.
- **A small corpus can invert the ranking between classes.** A 40-word slice of one reference
  grammar inverted the d4/d5 ranking measured on its full 236-word corpus — worse than the 12–28%
  sample-size distortion the original census plan warned about
  ([docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md](../superpowers/specs/2026-07-17-better-proposing-fst-plan.md#phase-0-results-2026-07-17-merged-328b5e8-verdicts-are-the-main-loops)). Below roughly 400
  signal-producing words, distrust a d1–d6 split ([.claude/skills/dead-end-census/SKILL.md](../../.claude/skills/dead-end-census/SKILL.md#2-run-the-census)).
- **Composition dead ends and enumeration dead ends are diagnosed by different tools that do not see
  each other's build path.** The standard census harnesses (`rust/crates/pg-foma-backend/examples/lab/worst_words.rs`,
  `rust/crates/pg-foma-backend/examples/lab/deadend_census.rs`)
  use the `emit::emit` path; a bug confined to a different network-construction
  route (e.g. the historical recipe-optimizer's deleted `build_controllable`, entry 6) produces no anomaly in a standard
  census run at all — confirm which construction path produced the numbers before trusting a clean
  census as a clean bill of health ([docs/fst-plan/large-lexicon-proposal-explosion.md](../fst-plan/large-lexicon-proposal-explosion.md#recommendation)).
