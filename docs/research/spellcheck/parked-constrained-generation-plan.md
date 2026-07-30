# PARKED — Constrained generation (predict the analysis, then generate the form)

**Status: PARKED.** Not scheduled for any current implementation phase. This plan exists so the idea
is not lost and so a future reader can un-park it correctly, without re-deriving the reasoning in
`17-constrained-generation.md`. Nothing in this document authorizes starting work. See that report for
full argumentation and citations; this document restates only what is needed to make a go/no-go
decision later, plus the concrete tasks, in enough detail that "later" does not mean "from scratch."

> ## Still parked as of 2026-07-30 — and report 27 is not an un-park (read this first)
>
> `27-prefix-constrained-fst-prediction.md` measured a **different mechanism** for the same goal:
> walking the compiled proposer network under the typed prefix and ranking completions from the tags
> each path already carries. It bypasses all three of this plan's prerequisites — the lazy enumeration
> engine, the trained tag-bundle predictor, and conformal calibration — so **none of its numbers
> satisfies this plan's un-park trigger, and none of them invalidates this plan either.** Two ideas,
> one goal; do not merge them, and do not treat a measurement of one as evidence about the other.
>
> **Two things in here to fix if this ever is un-parked:**
>
> 1. **The latency argument below prices the wrong operation.** "Nobody can afford an unbounded
>    generative search on every keystroke" is right about the *search*; the paragraph attributes the
>    cost to the analyzer `confirm`/HC call, which measured at **0.3-1.2ms** `[M, report 27]`. The
>    33ms Keyman budget affords tens of confirms, not one.
> 2. **Whatever is built, it is a separate top-k entry point, never a proposer mode** — `PLAN.md`
>    **D19**, decided from the proposer's over-approximation invariant. That applies to this plan's
>    idle-time variant as much as to report 27's walk.

---

## What is being parked, and why (read this paragraph and stop, if that's all you need)

D14 (`docs/research/spellcheck/PLAN.md`) decided that runtime generation for uncached words is shelved,
based on the project lead's traffic model: 90% of words are typed correctly and already in a shipped
warm cache, 9% are mistyped but the intended word is still in that cache, and roughly 1% are neither —
uncached misses that would need live, generative recovery. The lead's own words: **"if we miss the 1%
no one is sad."** Generation itself is not the problem — a bounded, offline, grammar-driven generator
already runs at pack-build time to fill the ~10k-entry warm cache (D14) — the problem is *keystroke-time
latency*: nobody can afford an unbounded generative search on every keystroke, and Keyman's own
correction-search budget (33ms, D8a) makes that concrete. "Constrained generation" is the idea that if
you could cheaply and reliably guess *which* stem and *which* feature bundle a partially-typed word
needs, you could generate just that one form (or a small set) instead of searching blindly — and the
idle-time variant of that idea (§ below) sidesteps the latency objection entirely by moving the work off
the keystroke path altogether. It is parked because none of its prerequisites exist yet, its expected
value depends on measurements nobody has taken, and it addresses a bucket of traffic the project lead
has already, explicitly, priced as low-stakes to miss.

---

## The un-park trigger

Un-park this plan only when **all three** of the following are true, not one or two:

1. **D14 (warm cache) and D4 (two-scale ranking) are both shipped and have real field telemetry** —
   not projected numbers, actual measured cache-hit rate and actual measured accept-rate-at-rank-1 from
   real usage. Building a generation layer to improve a number nobody has measured yet is building on
   sand.
2. **The residual "would-have-wanted-generation" miss rate is measured, not assumed, to be
   non-negligible.** The lead's 1% figure is a stated assumption, not a measurement (D14 flags it as
   such). If real telemetry shows the residual miss rate is smaller than 1%, or that it is dominated by
   genuinely novel stems (which no version of this plan can help with, per `17-constrained-generation.md`
   §5.7), the case for un-parking weakens further, not strengthens.
3. **A from-real-corpus test of tag-bundle-from-context prediction, at PanGloss's actual per-grammar
   gold-annotation scale (not a larger benchmark's scale), clears a stated accuracy/set-size bar
   against the cost arithmetic in Task 4 below.** Report 17 §2.1 found the honest expectation at
   Sena 3's scale (760 gold records) is in the 15–60% whole-bundle accuracy range, well below LEMMING's
   82–94% at its 100K-token floor — this must actually be measured per grammar, per POS, not inferred
   from a different language's benchmark.

If any of the three is unmet, the correct action is "keep parked," not "build a smaller version to see."

---

## Phased tasks

Each task names its prerequisite explicitly, and marks clearly whether that prerequisite exists today.

### Phase 0 — Measurement, before any new code

**Prerequisite: none — this phase can start with tools that exist today; it produces the evidence the
un-park trigger needs.**

- **0.1 — Per-grammar, per-POS tag-bundle prediction accuracy at real gold-data scale.** Using
  whatever FLEx interlinear gold annotation exists per grammar (Sena 3: 760 `WfiAnalysis` records,
  D13; smaller for the other three measured grammars), train and measure the simplest viable
  predictor — start with a linear-chain CRF or even a smoothed class n-gram (D4's own machinery,
  already decided) — for the specific sub-problem "given context, predict this token's POS, then,
  conditional on POS, predict its feature bundle" (report 08/17's per-POS decomposition). Report
  accuracy separately per POS category, matching report 13's own per-POS census format. **This uses
  existing tools (D4's n-gram machinery, once it exists per D15) plus the existing gold data — no new
  engine.**
- **0.2 — recall@k of the candidate generator, retried under D9 tier-1/2 assumptions.** Report 13
  found this unmeasurable because tier-1/2 generation does not exist. Once Phase 1 below builds even a
  minimal version, re-run this measurement — it directly answers "does ranking/prediction have a
  solvable problem here," per report 09's own framing.
- **0.3 — Template cross-product size measurement**, exactly as specified in `17-constrained-
  generation.md` §4.6: per grammar (the four report 13 measured, plus synthetic stress grammars), per
  POS, measure candidate-set size at each of the five constraint levels (unconstrained, POS-fixed,
  feature-bundle-fixed, conformal-set-intersected at k=1/3/5/10, prefix-filtered at increasing typed
  lengths). **Prerequisite: `pg_rules::stratum::synthesize_template` already exists
  (`pg-rules/src/stratum.rs:1331-1358`) and is directly callable for this measurement without any new
  public API — it just needs a harness, not new engine code, since this is a measurement task, not a
  shipped-capability task.**
- **0.4 — Idle-time compute/battery/storage cost, on a named low-end reference device.** No such
  device has been chosen yet (report 11's own unresolved item). Reuse `openspec/changes/
  calibrate-fst-resource-envelopes`'s harness, at the idle-batch workload shape rather than the
  keystroke-latency shape it currently measures. **Prerequisite: the harness exists; the reference
  device does not — naming one is itself part of this task, shared with report 11's outstanding item,
  not a separate decision to make twice.**

### Phase 1 — The missing API (only after Phase 0 clears the bar)

**Prerequisite: none of this exists today — confirmed by direct code survey, `17-constrained-
generation.md` §6.**

- **1.1 — Feature-bundle-to-rule-set resolution.** A function that, given a `FeatureStruct` (or a
  small set of them), resolves which `SlotDef.rules` entries in a POS-selected `AffixTemplateDef` are
  consistent, using the existing `unify`/`is_unifiable`/`subsumes` primitives
  (`pg-featstruct/src/tree.rs`). **Prerequisite: the underlying unification primitives exist; the
  resolution function does not.**
- **1.2 — Lazy, budget-capped, prefix-aware enumeration.** Promote `pg_rules::stratum::
  synthesize_template`/`synth_slots_generic`'s shape to a lazy `Iterator`, parameterized by (root,
  POS/template, target bundle or conformal set from 1.1, typed-prefix string, budget), with
  early-abort pruning at each slot step against the typed prefix, and a hard cap in the style of
  `pg-foma/src/compose_budget.rs`'s existing `HC_COMPOSE_*` env-tunable, checked-before-the-expensive-
  step convention. **Prerequisite: the eager, unconditional version of this walk exists
  (`pg-rules/src/stratum.rs:1331-1486`) and is test-only/unreachable today; the lazy, filterable,
  prefix-aware version does not exist.**
- **1.3 — Confirm-gating.** Every candidate the lazy walk produces must still pass
  `Morpher::parse_word_selected`-style verification before being offered, preserving the propose→confirm
  invariant (`CONTEXT.md:195-196`) exactly as `pg-foma::confirm` already does for the analysis
  direction. **Prerequisite: the confirm machinery exists (`pg-foma/src/confirm.rs`); wiring the new
  generation-direction walk through it does not.**
- **1.4 — Public surface.** Expose 1.1–1.3 as a new `pg-parse::Morpher` method, alongside
  `generate_words`/`generate_words_from_analysis`, per `17-constrained-generation.md` §6.3's crate-home
  reasoning (spans `pg-rules` + `pg-parse`; no new crate).
- **Constraint on this phase, inherited from D15 and verified against the code
  (`17-constrained-generation.md` §6.4): must not create any pressure to collapse `Vec<WordAnalysis>`
  to a single best analysis anywhere else in the analyzer.** This is additive, independent surface,
  the same posture `pg-realize` already takes for its own additive gloss layer.

### Phase 2 — Tag-bundle prediction + conformal calibration (only after Phase 0.1 clears the bar)

**Prerequisite: D4's class-backoff LM exists (D15 names this as the top prerequisite for the whole
Layer-2 add-on generally, not specific to this plan) — this phase cannot start before D4 does, since
Phase 0.1's own measurement already needs D4's machinery.**

- **2.1 — Per-POS predictor, not a flat whole-bundle predictor.** Per report 17 §2.2 and report 08's
  own recommendation, build POS-conditioned feature-bundle prediction, not joint POS+feature
  prediction in one shot — this is a design choice with direct measured support (Horsmann & Zesch
  2016), not a default to reconsider per grammar.
- **2.2 — Conformal calibration, per-POS, RAPS-regularized.** Calibrate prediction **sets**, not point
  predictions, using split conformal prediction (report 17 §3.2) with RAPS-style regularization (§3.3)
  to control set-size blowup in the large-label-space regime PanGloss's tag-bundle space sits in.
  Calibrate separately per POS category — the same decomposition that makes prediction itself
  tractable makes calibration tractable too, since it lets each POS category's calibration set be
  drawn from wherever that category's gold examples actually are, rather than diluting a single
  calibration set across categories with very different feature-bundle richness (report 13's own
  finding). **Prerequisite: none of this exists; the primitives (conformal calibration itself) are
  generic statistics, not PanGloss-specific engine code, so this is a smaller build than Phase 1.**
- **2.3 — Domain-shift mitigation for calibration data, or an explicit acknowledgment that it is
  unmitigated.** Per report 17 §3.3(a), any calibration set drawn from FLEx interlinear text or
  Scripture/Paratext text is plausibly out-of-domain relative to live keystroke context — the same
  caveat D15 already states for the class LM generally. Either apply a covariate-shift-aware
  reweighting (Tibshirani et al. 2019) if a characterizable shift can be estimated, or ship with an
  explicit, stated degradation of the "90%" guarantee, never a silent one — the same "honest error
  beats silent failure" standing rule this whole research series already follows.

### Phase 3 — Idle-time personalized generation (only after Phase 1 and Phase 2 both ship)

**Prerequisite: Phase 1's lazy generation API and Phase 2's per-user-observable tag-bundle signal both
need to exist. D8b's observation mechanism (context flows through `predict()` on every keystroke
already) exists today and needs no new work.**

- **3.1 — Idle-trigger wiring**, per report 17 §5.3: device idle + charging, or an accumulated-pairs
  threshold, or a periodic cap — whichever the Phase 0.4 battery/storage measurement supports.
- **3.2 — Observed-cell tracking and nearby-cell selection.** From the stems and tag bundles a user has
  actually produced (via `context.left`, already flowing per D8b), identify nearby unobserved paradigm
  cells for the same stem, bounded per D14's own "budgeted sample, never the inventory" argument.
- **3.3 — In-worker, evictable storage**, reusing the same `IndexedDB` mechanism and regenerable/
  authored distinction D8b already establishes for the tier-0 seen-word cache — no new storage
  mechanism, no new privacy surface (report 17 §5.5).
- **3.4 — Ranking rung**, per report 17 §5.6: insert as a new rung between the shipped warm cache and
  any future keystroke-time/on-demand generation, using D9's existing large-fixed-penalty mechanism,
  never a learned weight for the tier boundary itself.

---

## Measurements that must be taken before and during, with metric definitions

| Metric | Definition | When |
|---|---|---|
| Tag-bundle prediction accuracy, per POS, per grammar | Fraction of held-out gold tokens whose predicted feature bundle (conditional on correctly- or gold-given POS) exactly matches the annotated bundle, computed separately per POS category | Before (Phase 0.1); re-measured after any predictor change |
| Conformal set size at target coverage | Average and p90 cardinality of the RAPS-regularized 90%-coverage prediction set, per POS, per grammar | During Phase 2.2 calibration; re-measured whenever the calibration set changes |
| Template cross-product size at each constraint level | Count of distinct slot-rule combinations surviving at each of the five levels in report 17 §4.6, per (grammar, POS) | Before (Phase 0.3); re-measured if the grammar's templates change |
| Idle-batch wall-clock, peak RSS, battery draw | Measured via `calibrate-fst-resource-envelopes`'s harness at the idle-batch workload shape, on the named reference device | Before (Phase 0.4); re-measured on every generation-engine change |
| Residual "would-have-wanted-generation" miss rate | Fraction of real (not synthetic) keystroke sessions where tiers 0/1 (cache + error-tolerant cache search) came up empty and the user's eventually-accepted word was neither | After D14/D4 ship, from field telemetry — this is the un-park trigger's own metric, not a plan-internal one |
| recall@k of tag-bundle-conditioned generation | Given the true (root, feature bundle), does the top-k conformal set's generation include the form the user actually typed | During Phase 0.2/2.2, and continuously once shipped |

---

## Failure modes and what each would look like

- **Domain-shift miscalibration (report 17 §3.3a).** What it looks like: the "90%" prediction set
  actually covers the truth at, say, 60% on live keystroke text, because the calibration data (FLEx
  interlinear or Scripture text) does not resemble typing context. Detectable by holding out a small
  slice of whatever real usage telemetry exists (once D14/D4 ship) and checking realized coverage
  against the nominal target — this is the same discipline report 09 already recommends for any
  held-out-from-synthetic evaluation (never trust the number computed against the same distribution
  the model was calibrated on).
- **Set-size blowup on thin-data POS categories (report 17 §3.3b).** What it looks like: for a POS
  category with few gold examples in a given grammar (e.g. Sena verbs' 16.9% feature-population rate,
  or any grammar's minority POS), the conformal set balloons to include most or all of that category's
  tag-bundle space, at which point generating "the smallest set" is barely smaller than generating
  everything — the constraint has stopped doing work for that category specifically, even though
  coverage validity is intact. Detectable directly from the Phase 2.2 measurement above; the mitigation
  is not more conformal cleverness, it is falling back to POS-only generation for that category
  (report 17 §4.2's finding that POS alone is often the only real constraint available for
  low-`syn_fs`-richness categories).
- **The idle-time mechanism silently oversold as "solving unseen words."** What it looks like: someone
  reads this plan later, sees it shipped, and assumes the general uncached-word problem is closed. It
  is not — report 17 §5.7 is explicit that this only ever helps paradigm-neighbors of an already-typed
  stem. The mitigation is procedural, not technical: whatever telemetry or documentation describes this
  feature once built must state its scope (paradigm-neighbor coverage only) rather than "unseen-word
  generation," so a future reader does not over-attribute credit to it or under-invest in the genuinely
  novel-stem case it cannot touch.
- **Battery/storage regression on low-end devices.** What it looks like: the idle-time job runs longer
  or draws more power than Phase 0.4's measurement predicted, because a real device's thermal/CPU
  throttling under sustained idle-charging load differs from the measurement rig. Mitigation: the same
  "honest error beats OOM/hang" standing rule — a hard wall-clock and step budget on the idle job,
  exactly like `pg-foma/src/compose_budget.rs`'s existing discipline, with a stated degraded mode
  (skip this cycle) rather than an unbounded run.
- **The lazy enumeration API (Phase 1) gets built, but nobody wires confirm-gating in correctly, and it
  starts proposing ungrammatical forms.** What it looks like: a generated form that the grammar
  wouldn't actually confirm surfaces as a suggestion. This is precisely the propose→confirm invariant
  (`CONTEXT.md:195-196`) already governs for every other path in the engine; the mitigation is that
  Phase 1.3 is not optional polish, it is the same non-negotiable gate every other generation/proposal
  path already has, and any implementation review should treat a missing confirm-gate on this new path
  as a correctness bug, not a style nit.

---

## What in `PLAN.md` this plan depends on — a change to any of these invalidates it visibly

- **D9** (tiered candidate supply, unseen-forms-ranked-below-seen). If D9's tier structure changes —
  in particular if the large-fixed-penalty mechanism is ever replaced with a learned weight — Phase
  3.4's ranking-rung recommendation must be re-derived, not just re-applied.
  Note ***also*** D9's amendment by D14 into "shelved at runtime, relocated to build time" — if D14
  is ever revisited (see next item), the whole premise of what is "already shipped" versus "what this
  plan adds" shifts.
- **D14** (warm cache ships; runtime generation shelved; the 90/9/1 traffic model). This is the single
  most load-bearing dependency — the entire justification for parking (rather than building) rests on
  D14's 1%-is-fine traffic model. If the traffic model is revised (a different lead judgment, or field
  telemetry contradicting it), the un-park trigger's condition 2 changes accordingly, and this plan's
  priority should be re-evaluated, not silently carried forward with stale numbers.
- **D15** (Layer 2 is a corpus-trained add-on, not part of the analyzer pack; D4 must exist before
  Phase 0.1/2 can run; the analyzer must keep returning `Vec<WordAnalysis>`, never a single best). If
  the multi-FST rewrite (referenced throughout `PLAN.md` § D13) ever adds an internal disambiguation
  or best-analysis step, Phase 2's fractional-count-style tag-bundle training breaks silently, exactly
  as D15 itself warns for D4. This plan inherits that exact risk, unmodified.
- **D4** (the two-scale class n-gram; per-POS rung selection). Phase 0.1 and Phase 2 both reuse D4's
  machinery directly (n-gram counting infrastructure, per-POS backoff-rung selection). If D4's design
  changes shape (e.g. the backoff ladder is redesigned, or the per-POS selection rule is dropped),
  Phase 0.1/2.1's "reuse D4's machinery" assumption needs re-checking, not just re-running.
- **D10/report 11** (per-grammar calibration harness, p90 latency metric, the still-unnamed reference
  device). Phase 0.4 depends directly on the harness and, more concretely, on a reference device
  finally being named — if that naming happens for other reasons before this plan is un-parked, Phase
  0.4 inherits it for free; if it never happens, Phase 0.4 cannot run at all.
- **Report 13's per-grammar, per-POS census** (rung cardinality, `syn_fs` population rates, ambiguity
  distributions). This plan's Phase 4/§4 cost arithmetic and Phase 2.1's per-POS design both cite
  report 13's specific numbers as evidence that the per-POS decomposition is the right shape for
  PanGloss grammars specifically, not just in the general tagging literature. Report 13 itself already
  flags that its own numbers are pre-multi-FST-rewrite and partly small-corpus/directional (Amharic/
  Indonesian/Aweti) — this plan should be re-measured against report 13's stated successor measurement
  (re-running `spellcheck_measure.rs` against the new topology) before Phase 0 work is trusted at face
  value.
