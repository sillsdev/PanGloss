# Review: is the spellcheck plan's measurement apparatus valid? — D16 point 5 audited

Independent methodology audit, run in parallel with sibling reviews (this session did not read
`REVIEW-LOG.md`, per instructions). Scope: not "is the design good" but **can any claim in this
plan actually be checked**, and specifically: does `research/`'s synthetic-sweep apparatus answer
the questions D16 assigns it, or does it answer a different, easier question and let the plan
believe it answered the hard one.

Evidence tags: `[M]` verified by reading a file in this repo (file:line given); `[A]` attested
externally with a checkable citation; `[S]` my own reasoning, flagged as such. Line numbers cite
`docs/research/spellcheck/PLAN.md` as of 2026-07-25; **the file grew from 1880 to 2025 lines while
this audit was in progress** (D18 and the Candidate ledger were inserted between D17 and "What data
we need" during this session) — line numbers below were re-verified against the live file after
that shift, but treat any PLAN.md line citation as approximate if the file has moved again since.

---

## 1. Verdict table

| # | Item | Verdict |
|---|---|---|
| 1 | Can synthetic corpora answer the § "What data we need" questions? | **SOUND WITH CAVEAT for 2 of 8 rows; INVALID/PSEUDO-ANSWERABLE for 4 of 8; UNSPECIFIED (apparatus doesn't exist yet) for 2 of 8.** See three-way table, § 3. |
| 2 | Does the generator produce morphology, or its statistical shadow? | **INVALID as a stand-in for real agglutinative morphology.** It is a controllable statistical shadow: no morphotactic template, no paradigm-cell consistency, no allomorphy, and analyses within a token's lattice are drawn from unrelated classes rather than correlated readings of one form. Valid only for questions that are about *lattice shape and corpus-scale arithmetic*, not about morphological structure. `[M]` |
| 3a | Correction metric specified? | **SOUND WITH CAVEAT.** `recall_at_k`/`mrr` exist and are correctly separated from candidate-generator recall in the harness's own documentation, but PLAN.md's prose still doesn't name acc@1 / MRR / recall@k as the three required numbers in one place, and D9's "recall@k of the candidate generator" was **found not buildable** (PLAN.md:1287-1299, report 13) — the metric exists in code, the underlying capability it measures does not. |
| 3b | Prediction metric (KSR) specified, including its trap? | **SOUND.** KSR is implemented (`eval/metrics.py:50-106`) and the +610ms/suggestion trap is correctly cited (`arXiv:2101.09157`, verified below) as a reason KSR alone is insufficient. What's missing is a paired *time* metric — the harness has no selection-time or suggestion-count field to actually catch the trap it cites. |
| 3c | Flagging metric specified? | **UNSPECIFIED, now partially addressed by D18** (added mid-session, PLAN.md:1830). D18 fixes the *mechanism* (never flag on cache miss) but still names no metric — C6 in the Candidate ledger (PLAN.md:1903) asks for "false-alarm rate," not precision/recall of detection, and no baseline is named. |
| 4 | Acceptance bars — form and baseline named? | **UNSPECIFIED for all of D13/D5/D17**, as the task brief states. The Candidate ledger (added mid-session) supplies deciding *measurements* for 7 questions but not one **number**. This is defensible under D17 (elimination carries the burden of proof, not deferral) but the report supplies the missing bar-*forms* in § 6. |
| 5 | Is weight stability a valid proxy for weight correctness? | **INVALID as currently framed.** Stability is necessary, not sufficient; a grid search can converge to a stable wrong optimum on a small, non-representative validation set. PLAN.md's own ask ("how large a gold set must be before grid-searched weights stop moving") measures the wrong quantity. |
| 6 | Is p90 single-stream the right latency percentile? | **SOUND WITH CAVEAT.** Correctly sourced as a named convention (MLPerf Mobile), correctly *not* oversold as an optimum — 11-latency-policy.md says so itself (`:338-353`). The gap is the reference device, honestly flagged as open, and the percentile choice for the *flagging* path (D18) may need to differ from the *suggestion* path. |
| 7 | Single highest-value experiment before real data | See § 7 — recommend the **silver-set-projection + weight-recovery experiment** (already half-proposed in PLAN.md's own round-2 proposal 4), extended into a controlled recovery test, not a stability test. |

---

## 2. The single most important finding, argued at length

**D16 point 5 says: "generate the shape synthetically and sweep it rather than reading a value off
a sample." The harness that exists today can do the "sweep" half of that sentence for exactly one
model family — a surface-word trigram — and D4's actual design, the two-scale class n-gram that is
the thing every open question in § "What data we need" is actually about, has not been built in
`research/` at all.**

This is not a design criticism (the report brief for the harness explicitly scoped it that way,
`18-research-harness.md:494-496` — "This harness is the infrastructure the sibling reports' model
families plug into; it deliberately ships only the weakest baseline"). It is a **measurement
validity** problem, because PLAN.md's § "What data we need" reads the existence of the harness as
license to say a question is answerable *today*:

> "At what corpus size does each backoff rung become estimable? ... a **sweep**: 10^3, 10^4, 10^5,
> 10^6 tokens of the same language | none | one complete grammar, full feature inventory"
> (PLAN.md:1920)

and D16 itself says:

> "Every question of the form 'at what corpus size does rung *k* beat rung *k+1*' is answerable
> *today* on synthetic data across a range" (PLAN.md:1724-1726 area, D16 point 5's elaboration)

Read literally, this is false as stated, and it is false for a reason worth separating into two
independent problems, both real:

**Problem A — the apparatus doesn't exist yet.** `research/src/spellcheck_research/models/` has
exactly one model, `StupidBackoffNgram` (`research/src/spellcheck_research/models/
ngram_baseline.py:41-110`), a plain surface trigram with no notion of POS, `syn_fs`, or a backoff
rung at all. There is no rung-1..rung-6 class n-gram anywhere in `research/`. So "at what corpus
size does rung *k* become estimable" cannot be swept *today* — there is nothing in the codebase
that has a rung *k* to become estimable. This is a scoping gap, closeable by ordinary engineering
(build the class n-gram against the same `SpellcheckModel` interface), and PLAN.md's own round-2
proposals already point at adjacent work (proposal 1, the lemma term) without naming this one
directly. **It should be named directly**, because right now the plan's prose and its actual
capability have drifted apart: the prose says "answerable today," the repository says "answerable
once the rung-*k* model is built."

**Problem B — even once built, the sweep would be measuring the generator, not corpus-size
sensitivity in general.** This is the deeper issue and the one D16 does not anticipate. Read the
generator's actual ambiguity mechanism (`research/src/spellcheck_research/synthetic/
generator.py:132-134`):

```python
def _sample_ambiguity_count(rng: np.random.Generator, mean: float) -> int:
    lam = max(mean - 1.0, 0.0)
    return 1 + int(rng.poisson(lam))
```

and the class-assignment mechanism for a token's classes across a sentence
(`generator.py:172-176`, a Markov chain over classes with Dirichlet-drawn transition rows). A
"rung" in D4's real sense is a *feature-subset projection of a linguistically-authored `syn_fs`
bundle* — POS+`syn_fs`, POS+feature-subset, POS+`mpr`, POS alone. In the generator, "feature
richness" is a single Bernoulli draw per analysis (`feature_richness`, `generator.py:120-129`) over
a fixed 3-feature pool (`FEATURE_POOL`, `profiles.py:61-65`) that has **no rung structure at all** —
there is no encoded relationship between a coarser and finer feature subset, no unification lattice,
no unequal feature cardinality across POS categories (the exact effect report 13 measured as the
*actual* finding — richness concentrates per-POS, `PLAN.md` D4 revision). So a sweep over this
generator cannot discover "at what corpus size does rung 3 (POS+feature-subset) separate from rung 5
(POS alone)" in any sense that transfers to a real grammar's rung 3, because the generator's notion
of "feature-rich" has no rung-3-shaped internal structure to begin with — it would have to be
purpose-built with an actual nested rung hierarchy (a POS with sub-features that unify to coarser
ones) before the sweep could even ask the right question. **Building that hierarchy requires
deciding, in advance, what a rung-3 feature subset looks like — which is exactly the open,
unresolved, per-grammar, per-POS selection problem PLAN.md itself names as unsettled (D1 "Open,"
D4's "the rung-3 selection rule changes: per-POS, not per-grammar," PLAN.md:1774-1780 area).** So
the sweep's answer would be conditioned on a rung-3 definition chosen by whoever builds the
generator extension — which is precisely the circularity risk the audit brief warns about: "measuring
'when does rung k become estimable' on it may only recover the generator's own assumptions."

**Why this matters more than a normal scoping gap.** D17's whole framing is that *elimination*
carries the burden of proof and a sweep's job is to *discriminate between 2-3 live candidates*
(PLAN.md:1817-1819, "a sweep that cannot separate them is not worth running"). A sweep run against
today's harness, on today's model, cannot discriminate between any of D4's rung candidates — because
none of them exist in the harness — and it is honestly reported that way in the harness's own docs
(`18-research-harness.md:494`, "the class/factored n-gram D4 actually specifies plugs into the same
`SpellcheckModel` interface as future work"). The risk is not that anyone has lied about this — the
report is honest — the risk is that PLAN.md's D16/D17 prose, written slightly later and at a higher
level of abstraction, states the capability in the present tense ("is answerable today") when the
correct tense is "will be answerable once the rung-*k* model exists, and even then the sweep answers
a question about the generator's assumed rung-3 encoding, not about rung-3 selection in general."
That gap between stated and actual capability is the single most important finding of this audit,
because every other answerability judgment in § 3 below inherits it.

**The corrective is cheap and already implied by the plan's own logic.** D17 says a sweep's
acceptance criterion is "discriminate between the live candidates." Applied to the Candidate ledger
itself (PLAN.md:1896-1904, added mid-session and a genuine improvement — see § 6), most rows *do*
have a well-defined synthetic experiment available now (C1: EM vs. fractional counts on the existing
ambiguity-lattice generator; C2: smoother comparison on the existing count structure; C7: class
trigram vs. surface trigram, which the harness can run today because both models exist). The rows
that cannot yet be run are exactly the ones that need a not-yet-built rung-aware model (C3, and the
"backoff rung" row of § "What data we need"). **Recommendation: split "What data we need"'s rung
question into two rows** — one for "does *a* class-conditioned term beat the surface trigram" (C7,
runnable today) and one for "does rung *k* separate from rung *k+1* for a specific, named rung-3
encoding" (not runnable until the rung hierarchy is built into the generator and a matching model is
built into `research/`) — so the plan stops treating them as one already-answered question.

---

## 3. Three-way answerability table for § "What data we need" (PLAN.md:1908-1927)

Columns: **Genuinely answerable synthetically today** (the harness as it exists can produce a
number that transfers to the real question); **Pseudo-answerable** (a number comes out, but it is a
property of the generator's chosen parameterization, not of the phenomenon named); **Not answerable
without real data** (no synthetic construction closes the gap, regardless of engineering effort).

| Row (PLAN.md:1919-1926) | Verdict | Why |
|---|---|---|
| Does the intra-word morpheme n-gram beat a surface n-gram? | **Pseudo-answerable** | The generator's affixes are drawn from a small per-class pool independent of any real phonotactic/morphotactic constraint (`generator.py:97, 106-113` — affixes are opaque string codes, concatenated with no template, no slot consistency across stems in the same paradigm cell — see § 4). A win here shows "an n-gram over recurring atomic tokens beats one over unique composite tokens," which is true **by construction** of how the generator builds surface strings (stem+affixes vs. affix codes alone) and would be true of *any* generator built this way, real or synthetic. It is not evidence about real morpheme-boundary statistics. `[M]` |
| At what corpus size does each backoff rung become estimable? | **Not answerable today; UNSPECIFIED whether pseudo- or genuinely answerable once built** | See § 2 — no rung-aware model exists in `research/`, and building one requires pre-deciding a rung-3 encoding, which risks recovering the generator's own assumption (pseudo-answerable) rather than a general fact. `[M]` |
| Is a lemma/stem term worth its weight? | **Not answerable without real data** | The question is explicitly about **attested-lemma cardinality** in a real lexicon (PLAN.md's round-2 proposal 1 names this the missing number) — a synthetic stem pool of arbitrary size (`types_per_open_class`, `profiles.py:49`) can be dialed to produce whatever answer is wanted. There is no real-world Zipf-over-lemmas distribution to calibrate the knob against yet. `[S]` |
| Does a phrase table beat a general n-gram on the same data? | **Pseudo-answerable, with a caveat that helps** | Collocation strength (Dunning log-likelihood) needs genuine cross-word collocational structure, which the generator does supply via its Markov-chain-over-classes mechanism (`generator.py:171-177`) — but only at the *class* level, not the *lexical* level a real phrase table exploits (idiomatic pairings of specific stems, not classes). A synthetic win here would show the harness's plumbing works, not that phrase mining beats n-grams on real text. |
| Do cross-word phonological effects help at all? | **Not answerable without real data** | Report 18/round-2 finding 4 already says this from the other direction: raw word-edge phonology blows the state space (44²/417² edge pairs against far fewer confirmed analyses) — the generator has **no phonological representation at all** (affixes are opaque codes, not phoneme sequences), so it cannot even pose this question, let alone answer it. `[M]` |
| Can tag bundles be predicted from context for constrained generation? | **Pseudo-answerable at best** | This is explicitly the "annotation-hungry" row requiring gold/silver token-level labels (PLAN.md:1924) — the generator can produce as much labeled synthetic data as wanted (every token comes with its generating class attached, `generator.py:180-186`), so a synthetic accuracy number is easy to obtain and easy to make arbitrarily high by tuning `class_transition_concentration` (`profiles.py:44-47`) toward peaked/predictable transitions. It answers "can the constrained-generation *architecture* be wired up," not "can it hit any particular accuracy on real morphosyntax." Real numbers here (CoNLL-SIGMORPHON, PLAN.md round-2 finding 6) already exist and are the ones that matter; the synthetic version cannot beat or validate them. |
| Are class-LM scores comparable across languages? | **Not answerable without real data** | This is a question about whether two *different real grammars'* class inventories and score scales are commensurable. A synthetic generator run twice with two different profiles produces two corpora that are commensurable **by construction** (same code, same feature pool, same scoring convention) — the synthetic version of this experiment cannot fail in the way the real question could fail, so a "yes" from synthetic data is close to meaningless here. `[S]` |
| What is the real ambiguity distribution? | **Not answerable — and PLAN.md already says so in its own words** ("ambiguity on an incomplete grammar measures incompleteness, not ambiguity," PLAN.md:1926). The synthetic generator's ambiguity is a chosen Poisson parameter (`generator.py:132-134`), so running it cannot tell you anything about what real ambiguity looks like; it can only tell you how a given *model* behaves *given* an assumed ambiguity level — useful for the model-comparison purpose the harness was built for, useless for this specific question as phrased. |

**Summary: 0 of 8 rows are genuinely, fully answerable synthetically today in the sense of producing
a number that transfers to the real-world question.** One (phrase table) is close, with the
class/lexical caveat noted. This does not mean the harness is worthless — see § 2's closing
recommendation and the Candidate-ledger cross-check — it means **D16 point 5's blanket claim needs
a per-question qualifier**, and § "What data we need" should carry the pseudo-/not-answerable
distinction explicitly rather than implying uniform synthetic tractability.

---

## 4. Per-item detail

### 4.1 (repeated from § 2/3 for completeness) — synthetic answerability

Already argued at length in § 2-3. One additional point: the generator's own documentation is
unusually honest about this already. `generator.py:36-42` states outright: "a token's *distractor*
analyses reuse a different class's morpheme/stem data... this generator does not attempt to make a
distractor's morphemes spell the shared surface." That is the author of the code naming exactly the
gap this audit is flagging — the harness was built candidly, and the risk is entirely in how PLAN.md
*talks about* what the harness can do, not in the harness's own self-description. `[M]`

### 4.2 Statistical shadow vs. morphology

Read directly, `generator.py` produces:

- **No morphotactic slot template.** `_build_vocab` (`generator.py:82-117`) builds a surface form as
  `stem_code + "p{p}" + "".join(chosen_affixes)` (`:111`) — every generated word is stem-then-suffix,
  with affixes drawn as an **unordered multiset** from a flat per-class pool
  (`rng.choice(affix_pool, size=n_affixes, replace=True)`, `:107`) and then joined in draw order.
  There is no prefix, no infix, no circumfix, and critically no *slot* concept: the same paradigm
  index `p` for two different stems in the same class gets an **independently drawn** affix set —
  nothing ties "paradigm cell 2" to a consistent affix (or affix + allomorph) across the class. Real
  agglutinative paradigms are defined by the opposite property: the same grammatical cell uses the
  same morpheme (subject to allomorphy) across every stem in an inflection class — that is what
  makes a paradigm a paradigm, and what a real class n-gram exploits. `[M, generator.py:82-117]`
- **No allomorphy / phonological alternation.** Affixes are opaque placeholder codes (`fx3_2`) string-
  concatenated verbatim; nothing in the generator models a stem-final or affix-initial segment
  changing shape at a boundary. `[M]`
- **Analyses within one token's lattice are not correlated paradigm-cell readings.** A token's
  "distractor" analyses are drawn from an **independently sampled random class** for the whole
  analysis, not a same-root or same-paradigm neighbor of the true reading
  (`generator.py:189-201`: `d_class_idx = rng.integers(0, n_classes)`, fully independent of the true
  analysis's class). Real cross-word/cross-analysis ambiguity is overwhelmingly the opposite —
  competing analyses of one surface form usually share a root and differ only in a feature or two
  (case syncretism, tense/aspect neutralization) — which is exactly the property a factored/class
  LM is built to exploit at the fine-grained rungs. A generator whose distractors are unrelated
  classes cannot exercise rung 2/3 discrimination the way a real ambiguous form would.
- **What it does model, correctly and usefully:** Zipf-skewed stem frequency within a class
  (`_zipf_weights`, `:76-79`), affixes recurring across many stems even though whole wordforms do not
  (the property D4/report 04 actually depend on for the intra-word term — `generator.py:17-20`
  states this explicitly as the target), a genuine cross-word Markov structure over *classes*
  (`:156-177`), and controllable ambiguity *count* (though not ambiguity *content*, per above).

**Conclusion for item 2:** the generator is well-built for what it claims to be — a *lattice-shape*
and *corpus-scale-arithmetic* stress test for a model-comparison pipeline — and its own docstrings
never claim to model paradigm structure, morphotactics, or allomorphy. The risk is entirely in
inference drawn one level up, in PLAN.md, where "the harness generates corpora with controllable...
morphological richness" (D16, PLAN.md:1707-1708 area) reads as if morphological *structure* were
modeled, when what is actually controllable is morpheme-sequence *length* and *recurrence rate*, not
structure. Any claim about backoff-rung behavior, paradigm generalization, or allomorphy-sensitive
tokenization is out of scope for what this generator can validate. `[M]`

### 4.3 Metrics

**(a) Correction.** `recall_at_k`/`reciprocal_rank`/`mrr` are implemented correctly and simply
(`eval/metrics.py:19-47`) and the harness's own docs are explicit that its correction-recall number
measures *ranking given a candidate set*, not *candidate-generator recall* (`harness.py:109-117`,
`13-first-measurements.md`'s own "What I could not measure" item 2, and `18-research-harness.md:484-
487`). That three-way separation (candidate-generator recall@k vs. reranker precision@1 vs. overall
accuracy) is exactly what report 09 §6 calls for (`09-training-without-data.md:546-562`) and it is
correctly implemented as a documented distinction, not silently conflated. **Gap:** PLAN.md's own
prose (D5's "minimum evaluation apparatus," PLAN.md~385-393) still doesn't collect "acc@1, MRR,
recall@k" into one explicit named metric triad the way report 09 does — a reader has to reconstruct
it from two separate documents. **The deeper gap, already self-reported:** D9's "recall@k of the
candidate generator, buildable now" claim was tested and found false (PLAN.md:1287-1299) — there is
no prefix-completion/error-tolerant-generation API in the Rust codebase, so the single most decisive
of the three numbers (does ranking even have a solvable problem) cannot be measured on real data
yet, and the Python harness's correction-recall number (which always injects the true word into the
candidate set, `harness.py:109-123`) cannot substitute for it — it is measuring a different, easier
question by construction, and says so in its own comment (`harness.py:116-117`).

**(b) Prediction / KSR.** Implemented per the standard word-completion definition
(`eval/metrics.py:50-106`), with OOV handled honestly (a token that's never predicted contributes
zero savings, not exclusion — `:71-74`). **Metric name to adopt: Keystroke Savings Rate (KSR)**, as
already used. The trap is correctly *cited* in PLAN.md ("suggestion selection time scales +610ms per
suggestion (R²=.98)," PLAN.md's round-2 finding 5, citing `arXiv:2101.09157`) — **independently
verified in this audit**: Buschek et al., "The Impact of Multiple Parallel Phrase Suggestions on
Email Input," CHI 2021 (`arxiv.org/abs/2101.09157`), reports exactly this regression (fitted on mean
search & selection times) and reports task-completion time **increasing** with more suggestions
(247s at 6 suggestions vs. 196s at 0) despite higher acceptance counts — a genuine, confirmed,
directly-on-point negative result `[A, verified via ar5iv fetch, 2026-07-25]`. **What's missing:**
the harness has no paired *time* metric at all — no simulated selection-time cost, no
suggestion-count field recorded alongside KSR. A harness that can report a KSR win without any way
to notice the trap it cites is citing the finding, not guarding against it. **Recommendation: add a
`mean_suggestions_shown` field and a synthetic `selection_time_estimate` (even a placeholder linear
model, `base_time + 610ms * (n_shown - 1)`) to every evaluation result**, so a future model
comparison cannot report a KSR win while silently increasing suggestion-list size in a way the
610ms/suggestion literature says will slow the user down.

**(c) Flagging.** At the time the audit brief was written, no decision addressed this at all. Mid-
session, **D18 was added** (PLAN.md:1830-1889), correctly ruling out "cache miss ⇒ flag" and naming
two legitimate triggers (a completed failed parse; an exhausted generative search). This is a real,
substantive improvement and resolves the *mechanism* question. **It still does not name a metric.**
The Candidate ledger's C6 row (PLAN.md:1903) asks for "false-alarm rate on correctly-typed complex
words," which is one half of the standard pair. **Recommendation: adopt precision and recall of
error detection explicitly** (precision = correctly-flagged / all-flagged; recall = correctly-flagged
/ all-actual-errors), report them **separately**, and treat false-alarm rate on correctly-typed
morphologically-complex words as the operational proxy for (1 − precision) specifically for the
population D18's own text identifies as most at risk (PLAN.md:1851-1853, "the long, richly-inflected
ones"). No baseline is named anywhere in the plan for this family; § 6 proposes one.

### 4.4 Acceptance bars

The task brief's premise holds: no number is attached to D13's certification bar, D5's "bar to
clear," or D17's discrimination requirement. What changed mid-session is that the **form** of several
bars is now implicit in the Candidate ledger (PLAN.md:1896-1904) even though the **value** is not:

| Decision | What form of bar does the ledger imply? | Value stated? |
|---|---|---|
| D5 (neural bar) | Beat-a-named-baseline (D4 itself, measured on the same split) | No number |
| D13 (certification) | Absolute threshold ("near 100%" coverage, John's own words, PLAN.md~1139) | "Near 100%" is stated but not defined (99%? 99.9%? per-token or per-type?) |
| C7 (inter-word unit) | Beat-a-named-baseline (surface trigram, "the floor everything must beat") | No margin stated — beat by how much, on what metric, to justify the added complexity? |
| C1/C2 (EM, smoother) | Beat-a-named-baseline at matched data size | No margin |

The one recurring baseline named consistently and correctly across the whole plan is the surface
n-gram — round-2 finding 2 explicitly calls it "a permanent diagnostic, never the ranking layer...
keep it in the harness forever as a floor: any model that cannot beat it is broken" (PLAN.md's
round-2 findings). That is the right instinct and the right baseline choice (supported independently
by the Finnish 20%-OOV-at-40M-tokens result and by report 09's finding that no neural model beats a
tuned classical baseline under ~100K labeled examples for any adjacent task, `09-training-without-
data.md:453-462`). **What is still missing is a stated margin and a stated form for every other
comparison** — see § 6 for concrete proposals per decision.

### 4.5 Gold-set sizing — is weight stability a valid proxy for weight correctness?

**No, and the plan's own literature review (report 09 §7) already contains the reasoning that
shows why, without drawing the conclusion.** PLAN.md's ask is stated plainly:

> "measure how large a gold set has to be before grid-searched weights stop moving, on synthetic
> data, and use that as the ask" (PLAN.md:1936-1938)

This conflates two different questions. **Stability** (do the weights stop changing as the gold set
grows) is a statement about the *variance* of the estimator. **Correctness** (are the weights close
to whatever the "true" optimal weights are) is a statement about *bias plus variance together*.
A grid search can converge — stop moving — while stuck at a value biased by:
- **Small-sample overfitting to the *specific* gold set's error/context distribution**, not the
  population of errors the shipped model will face. Report 09 §7 itself names the mechanism this
  audit is flagging, when discussing a different weight (λ_reranker): "the tuned λ will be biased
  toward however realistic the synthetic noise is" (`09-training-without-data.md:615-616`). The exact
  same bias applies to D4's interpolation weights tuned on a 147-token contextual gold set drawn from
  one Scripture-translation-checking export (`Sena_InterlinearTraining.fwdata`, per report 18) — a
  narrow domain and register. Weights can stabilize *perfectly* on that domain and still be wrong for
  general text.
- **Grid coarseness masquerading as stability.** If the grid step is coarse relative to the
  estimator's noise, weights will appear to "stop moving" simply because the grid has no finer point
  to move to — this is an artifact of grid resolution, not evidence of a converged true optimum.
  PLAN.md's ask does not distinguish these.
- **Low-dimensional search is genuinely more forgiving than the general small-validation-set
  literature** (report 09 §7 point 1 makes this argument correctly, and general ML practice agrees —
  `[A]` grid search over 1-3 scalars is a qualitatively easier regime than fitting a
  many-parameter model, and a small gold set can meaningfully constrain a 3-5-scalar search where it
  could not constrain a full model fit). This is a real mitigating factor and this audit does not
  dispute it. **But it bounds the *degree* of the risk, not its *existence*.** A 3-scalar search on
  147 points can still land on a value that overfits those 147 points' particular error distribution
  and domain register.

**What the literature actually recommends, and what this report found:** general searches on grid-
search-plus-small-validation-set overfitting converge on the same set of mitigations `[A, general ML
practice, not spelling-specific — searched directly for this audit]`: report robustness across
repeated random splits rather than a single point estimate; prefer (repeated) k-fold cross-validation
or bootstrap resampling over a single train/dev split at small N, specifically because repeated
splits reveal whether a "stable" point estimate is stable *across resamples* or merely stable
*because the grid stopped offering alternatives*; and always hold out a final check set the search
never touched. None of this is spelling/GEC-specific — no source was found addressing gold-set
sizing for interpolation-weight tuning in a low-resource morphological speller specifically, so this
recommendation is a transfer from the general small-N hyperparameter-tuning literature, not a
validated number for this task.

**Recommended actual procedure, replacing "measure when weights stop moving":**
1. **Bootstrap the existing/silver gold set** (resample-with-replacement B times, e.g. B=1000, over
   whatever token-level set exists — the 147-token Sena contextual set or its silver-projected
   expansion) and report the **distribution** of the grid-searched weight across resamples, not a
   single value. A tight distribution is evidence of stability; a wide one shows the "stopped moving"
   read was an artifact of a single split.
2. **Nested cross-validation** if/when the gold set grows past a few hundred tokens: outer folds for
   final accuracy, inner folds for the weight search, so the reported accuracy is never computed on
   data that influenced the weight choice.
3. **A synthetic recovery experiment, not a stability sweep** (this is the concrete fix to the D16-
   point-5 ask, and doubles as this audit's § 7 proposal): generate synthetic data from a profile with
   **known ground-truth optimal weights** (constructible because the generator's scoring convention is
   fully known), then grid-search on gold sets of size 50, 150, 500, 1500, 5000 tokens and measure not
   "did the search stop moving" but **"how far is the recovered weight from the known-true weight, and
   how does that error shrink with N."** This directly answers "how large a gold set must be" in the
   only sense that matters (recovery error, not motion), and it is buildable in the existing harness
   today — see § 7.

**Minimum defensible size, stated honestly:** no source found gives a spelling/GEC-specific minimum
for *interpolation-weight* tuning specifically (as opposed to general low-resource NLP annotation
sizing). Report 09 §6 cites 383 sentences/5,294 tokens (Malagasy) and 196 sentences/4,882 tokens
(Kinyarwanda) as *workable annotated training sets* for other low-resource NLP subfields
(`09-training-without-data.md:480-482`), and 50-400 hand-annotated sentences as report 09's own
recommended gold-set size for final reranker evaluation (`:688-689`) — both are training/evaluation-
set sizing precedents, not interpolation-weight-recovery precedents, and neither number transfers
directly. **Honest answer: unverified for this specific quantity; the recovery experiment in point 3
above is how to obtain a real one**, and until it is run, treat 147 (or its silver-projected
expansion) as adequate only for a low-dimensional sanity check, never as the basis for a weight
choice that ships.

### 4.6 Latency metric

**P90 single-stream is soundly sourced and honestly framed as a convention, not a proven optimum** —
`11-latency-policy.md:338-353` verified directly `[M]`: MLPerf Mobile Inference Benchmark (Janapa
Reddi et al., `arXiv:2012.02328`) states "Single-stream mode measures the 90th-percentile latency
over at least 1,024 samples for a minimum run time of 60 seconds," and the report itself says P90
"is not stated by MLPerf beyond 'single-stream' being the standard interactive-load scenario
definition — this is a case where the number is a de facto industry convention, not a derived
optimum, worth naming honestly." That is exactly right and needs no correction. `[A, verified: MLPerf
single-stream methodology is a real, citable convention]`

**What the audit adds:** the RAIL 100ms "Response" perceptual-threshold figure
(`11-latency-policy.md:296-298`, Google's `web.dev/articles/rail`, verified in-report as fetched
directly) is explicitly stated to carry **no percentile qualifier** — it is a perceptual constant,
not a distributional claim — which means P90-of-100ms and "100ms on average" are not the same
target, and the report does not reconcile them into one number. If PanGloss's real target is "feels
instantaneous," the honest reading of RAIL is closer to "P90 should itself be near 100ms," not "P90
of some larger nominal budget." This is worth stating as an open arithmetic question, not resolved
anywhere in the plan: **is the target P90 latency ≤ 100ms, or is 100ms itself the average/typical
case with a looser P90?** No source resolves this and PLAN.md doesn't pick one.

**Reference device — correctly flagged as unresolved** (`11-latency-policy.md:368-372`, verified:
even MLPerf Mobile's own methodology "does not name a canonical 'low-end' reference device"). The one
paper found testing a genuine budget device by name, Li et al. EdgeSys 2024 (Samsung Galaxy A03s),
yielded "no readable percentile/device-tier methodology text even through the reader proxy"
(`:52-55`) — so this report's own citation for a candidate device is itself unverified past the
device name. **A03s launched 2022 and remains a plausible reference point for "low-end Android" as of
2026** `[S] — not independently re-verified in this audit`; the plan should name it (or a successor
in the same market tier) explicitly rather than leaving "we name one" open indefinitely, since naming
*a* device, even provisionally, is what makes every other latency number in this plan checkable.

**A latency-percentile question the plan does not ask:** D18's flagging path and D9/D10's suggestion
path may need *different* percentiles. A missed suggestion (tier 0/1 slow) degrades gracefully per
the anytime contract (D10). A flagging decision that runs long (D18's "an attempted parse that
failed" requires the analyzer to actually run, PLAN.md:1875) has no anytime fallback named — either it
completes and flags/doesn't, or it times out and D18 says nothing about what a timed-out flagging
attempt should report. This is a gap this audit did not find addressed anywhere and is worth adding
to the Candidate ledger.

### 4.7 The one measurement that would most change the plan

See § 7 below.

---

## 5. What I could not verify

- **The CoNLL-SIGMORPHON 2018 Task 2 Track 2 numbers** (PLAN.md's round-2 finding 6: "best system
  38.60% against a copy-the-lemma baseline of 36.62%, neural baseline 2.19%") — PLAN.md's own
  verification note already flags this as unverified ("no PDF rendering available here"). I did not
  attempt to re-verify it either; flagging again rather than silently re-citing it as settled.
- **The exact minimum viable gold-set size for interpolation-weight tuning specifically** — searched
  directly for this audit; no spelling/GEC-specific or interpolation-weight-specific source was
  found. § 4.5's recommendation (the recovery experiment) is offered precisely because no existing
  number was found to cite instead.
- **A03s's continued relevance as a 2026 low-end-Android reference device** — not independently
  re-priced or re-benchmarked in this audit; flagged as `[S]` above, not `[A]`.
- **Whether the Filipino/Zarma/GBDT/gzip+kNN comparisons in report 09 §5 hold up on direct source
  read** — this audit did not re-fetch those papers; it relies on PLAN.md's and report 09's own
  citations, which that report tags `[M]`/`[A]` at varying confidence already. Not re-verified here.
- **Whether IndexedDB-under-`file:`-origin actually behaves as D8b assumes** — out of this audit's
  scope (not an evaluation-validity question) but noted because D8b's own text already flags it as
  unverified and this audit did not check it either.
- **The MAGEC 64.24/69.47 F0.5 figures** (cited in D5 and report 09) — not re-verified in this audit;
  PLAN.md's own sourcing note for report 09 already treats some of this report's numbers as
  search-snippet-level, not independently fetched.

---

## 6. Proposals for John

Each phrased as a concrete change to a named decision, or a named experiment with an acceptance
criterion, per the brief.

1. **Amend D16 point 5 with a per-question qualifier.** Change "generate the shape synthetically and
   sweep it" to explicitly distinguish *lattice-shape/corpus-scale-arithmetic* questions (genuinely
   synthetic-answerable) from *morphological-structure* and *cross-language-comparability* questions
   (not synthetic-answerable regardless of engineering effort, per § 3's table). Concretely: add a
   column to § "What data we need" marking each row genuinely-answerable / pseudo-answerable /
   not-answerable-synthetically, using § 3's table as the first draft.

2. **Build the rung-aware class n-gram in `research/` before claiming the rung-*k* sweep is
   runnable.** Named experiment: implement a `ClassNgram` model (POS+`syn_fs` down to POS-alone,
   D1's four surviving rungs) behind the existing `SpellcheckModel` interface, sweep it against
   `StupidBackoffNgram` across the four existing profiles at 10^3/10^4/10^5/10^6 tokens. Acceptance
   criterion: the class n-gram beats the surface trigram (already the standing floor per round-2
   finding 2) at every scale tested, by a stated margin (recommend: ≥10 percentage points of acc@1 at
   10^4 tokens, tightening as scale grows) — if it does not beat the floor at some accessible scale,
   that is itself a finding worth surfacing loudly rather than assuming the win.

3. **Add a paired latency/suggestion-count field to every KSR result**, closing the gap named in §
   4.3(b): `mean_suggestions_shown` and a `selection_time_estimate` derived from the 610ms/suggestion
   figure. Acceptance criterion: no model comparison in this project may report a KSR improvement
   without also reporting whether estimated selection time increased — a KSR win with a selection-time
   loss is a finding to surface, not to average away.

4. **Adopt named metrics for flagging (D18/C6) explicitly: precision and recall of error detection**,
   reported separately, with false-alarm rate on the "long, richly-inflected words" subpopulation
   (D18's own named risk group) as a required breakout, not just an aggregate. Acceptance criterion,
   proposed as a starting bar subject to revision once real data exists: **precision ≥ 0.9 on
   correctly-typed complex words specifically** (i.e., ≤10% false-alarm rate on exactly the population
   D18 worries about) before flagging (option A, PLAN.md:1880) may replace suggest-only (option B) as
   the default; this is a beat-a-floor bar where the floor is B's trivial zero-false-alarm baseline,
   made concrete rather than left as "the number that decides it."

5. **Replace the gold-set-sizing ask in § "What data we need" (PLAN.md:1936-1938) with the recovery
   experiment from § 4.5.** Concretely: generate a synthetic profile with known ground-truth optimal
   interpolation weights, grid-search on synthetic gold sets of 50/150/500/1500/5000 tokens (spanning
   the current 147-token reality and two orders of magnitude beyond it), and report weight-recovery
   error (distance from true weights) vs. N, with a bootstrap confidence interval at each N rather
   than a single point. Acceptance criterion: name the N at which the 90% bootstrap CI on each tuned
   weight is narrower than some stated operational tolerance (e.g., ±0.05 in a weight normalized to
   sum to 1) — that N, not "weights stop moving," is the actual ask to carry into "what data we need."

6. **Name a concrete reference device for latency, provisionally.** Recommend picking a currently-
   available budget Android device (this audit could not independently verify a specific current SKU
   as of 2026-07-25 and did not want to fabricate one) and stating explicitly whether the P90 target is
   ≤100ms (RAIL-perceptual framing) or a looser P90 around a ~100ms *typical* case — § 4.6's open
   arithmetic question. Acceptance criterion: any latency number reported in this project must state
   both the device and which of the two readings it targets, or it is not comparable across reports.

7. **Add a row to the Candidate ledger for the flagging-path latency question** (§ 4.6's last point):
   what happens when a D18-qualifying parse attempt times out — is a timeout treated as "attempted and
   failed" (flags), or as "inconclusive" (does not flag, per D18's own conservative spirit)? This is
   currently silent and composes with D10's anytime contract in a way D18 does not address.

8. **The single experiment to run before real data, if only one is chosen:** the weight-recovery
   experiment (proposal 5). Reasoning: it is the cheapest of the open items to build (the harness and
   generator already exist; it needs a known-weights synthetic profile and a grid-search loop, not new
   model code), it directly replaces an already-flagged-as-wrong measurement (stability-as-proxy) with
   a correct one, and its outcome changes a concrete near-term decision (how much text/annotation to
   prioritize acquiring for Layer 2, PLAN.md's own top-3 unknowns list, PLAN.md:1930-1938) rather than
   an abstract one. If the recovered-weight error is still large at 1,500-5,000 tokens, that is a
   direct argument for prioritizing the silver-set-projection extraction (round-2 proposal 4,
   PLAN.md:2011-2013) over any other Layer-2 work, because it says the 147-token reality is nowhere
   near sufficient and scaling the gold set is the actual bottleneck — a conclusion the current
   "did weights stop moving" framing cannot produce even in principle, because it never compares
   against a known answer.
