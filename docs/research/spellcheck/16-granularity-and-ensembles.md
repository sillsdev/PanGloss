# Conditioning granularity, and shipping several self-updating models at once

Report 16 in the spell-checking research series. Scope: two questions from the project lead —
*"Doing an n-gram over the stems? A different subset?"* and *"We will likely ship multiple at the
same time, ideally self-updating based upon what people type."* Extends report 04 (n-gram/factored
LM prior art), grounds every cardinality claim in report 13's measured numbers, and extends report
06 (personalization/privacy) into the specific self-updating design this plan needs. Binding: D1,
D4, D9, D10, D14, D15. Does not relitigate them. Series convention: `[M]` = measured directly in
this repo (report 13's harness or code read directly), `[A]` = asserted in a cited source (source +
number given), `[S]` = this report's own reasoning/extrapolation, not to be read as `[A]`.

## Verdict up front

**Four models ship simultaneously, all folded into D4's one unified log-linear composition, not
run as separate systems.** Ranked finest-to-coarsest signal, not finest-to-coarsest *cardinality*:
(1) the inter-word class trigram (D4, already decided, over the shortened 4-rung ladder — rungs 1
and 4 are measured-dead, report 13); (2) the intra-word morpheme n-gram (D4, already decided); (3)
a **new lemma/root bigram** — this report's answer to the lead's stem question, argued in §2, not
currently in D1's ladder because it isn't a coarsening of the same axis, it's an orthogonal lexical
axis the ladder structurally discards; (4) the **personal cache term**, which is D9 tier-0 +
D8b's in-worker accumulation *already being a cache language model* (Kuhn & De Mori 1990) that the
plan has not yet named as one (§5) — the single most actionable finding in this report, because it
means half of "self-updating" is already decided and merely needs recognizing.

**On the lead's specific question:** yes, a lemma rung is worth adding, but not as a rung — the
existing ladder coarsens *grammatical class*; lemma preserves *lexical identity* while discarding
*inflection*, which is a different variable entirely. It enters as a fourth additive term with its
own tiny internal backoff (bigram → unigram → skip), not as a fifth entry in D4's POS/`syn_fs`
hierarchy. No source found gives a controlled lemma-vs-POS-vs-surface comparison at small data for
a morphologically rich language — this is a real gap, flagged rather than papered over (§2).

**On tuning weights with no held-out data (D4):** the grid search is defensible for the shape D4
actually specifies — 3-4 scalars, not a trained MaxEnt/CRF with many features — and report 09's own
recommended apparatus already matches what D4 does (low-dimensional grid search, conservative
default). Mixture-of-experts (a learned per-context gate) is the scheme to rule out at this scale;
it needs a function fit, not a handful of scalars, and nothing in the gathered evidence supports
that being estimable from ~760 tokens (§4).

**On self-updating:** the honest state of the evidence is that cache-recency effects are old,
solid, and measured to matter (Goodman 2001: up to 0.6 bits perplexity from caching alone on small
data `[A]`) — but no source anywhere in this series gives a specific decay half-life, and no source
gives a per-keystroke cold-start adaptation curve for this task shape. Both are flagged as unknown,
each with the concrete experiment to run rather than a guessed number (§5, §6).

**On what must stay fixed:** D10's recall invariant and D9's unseen-form penalty are joined by a
short, principled list — the tier ordering itself, the admission gates (D12/D13), coverage, the
class-defining inventories D15 binds against, and (new) the shipped weights of the log-linear
composition itself, which should shrink toward, never freely drift from, their calibrated defaults
under on-device adaptation (§7).

---

## 1. Which unit should the inter-word term condition on?

D1 already enumerates the deterministic, load-bearing fields a factor can be built from
(`WordAnalysis`, `pg-parse/src/lib.rs:25-44`): `morpheme_ids`, `root_morpheme_index`, `pos_id`,
`syn_fs`, `mpr`. Root/lemma identity is *already* on D1's list as a derived factor — "root identity
and root frequency" (`PLAN.md` § D1, "Deterministic derived factors") — but D4's backoff ladder
never turns it into a conditioning unit. That gap is exactly what this section and §2 close.

**How lemma identity is actually recovered today, verified by reading the code.** There is no
`lemma_id` field on `WordAnalysis`. The identity is reachable by resolving the root morpheme:
`resolve_morpheme(g, MorphemeId(wa.morpheme_ids[wa.root_morpheme_index as usize]))` returns
`MorphemeOwner::Root(LexEntryId)` (`pg-parse/src/morpher.rs:1103-1105, 1310-1312`) — a linear scan
over `g.entries` (`morpheme.rs:1104`, "fine for generation... not the per-word analysis/synthesis
hot path" per its own doc comment). Two consequences worth flagging before any lemma-conditioned
model is built: (1) `resolve_morpheme` is a private `fn`, not exported from `pg-parse` (`lib.rs`'s
`pub use` list does not include it) — a lemma factor needs either a small export or a client-side
reimplementation of the same scan; (2) the doc comment's own caveat that the scan is "fine for
generation... not the hot path" is a real signal that resolving lemma identity **at LM-scoring
time, per candidate, per context word** is not the use its current implementation was sized for —
this is a cheap fix (cache `LexEntryId` alongside `WordAnalysis` once resolved, don't re-scan per
score) but it is a real, stated gap, not a non-issue.

### The candidate units, measured where measured

| Unit | What it conditions on | Type/token behaviour | Measured cardinality (Sena / Amharic / Indonesian / Aweti) | Estimable at 10⁴ tokens | 10⁵ | 10⁶ |
|---|---|---|---|---|---|---|
| **Surface wordform** | the exact typed string | Worst case — Turkish reaches 106,547 word types per 1M-token corpus `[A, report 04 §1, arxiv 2508.14292]`; Finnish word-level OOV measured at 20% `[A, report 04 §1, Hirsimäki et al. 2006]` | not separately measured here (this *is* D1's rejected rung 0 — report 04's whole starting complaint) | No — even English trigrams need 24-41% backoff at ordinary sizes `[A, report04 §1]`; morphologically rich languages are strictly worse | Doubtful for trigrams; bigrams marginal | Plausible for bigrams only, per the Turkish/Finnish numbers extrapolated `[S]` |
| **Lemma / root** (`LexEntryId` via root resolution above) | lexical identity, inflection stripped | Much smaller than wordform types by construction — one lemma spans every inflected surface form. **Attested-lemma cardinality is NOT measured** (needs running text; see §2's correction note). The only available bound is the grammar's `LexEntry` count — Sena 1,462, Amharic 130, Indonesian 66 `[M, report 13:181,205-206]` — which is an *upper* bound on lemmas attested in the 6,973 / 673 / 121-form type lists, not a ratio against them `[S]` | Not yet known — no report measures lemma-bigram class density directly | **Plausible** — lemma inventory is fixed by the *lexicon*, not the corpus, so it saturates fast; likely comparable to English word-level bigram behaviour (workable but not dense) at this scale `[S]` | Likely workable, by the same reasoning `[S]` | Likely dense `[S]` |
| **Morpheme sequence** (bare, no features) | the realized slot/template pattern | Morphemes recur across words even when wordforms don't — Finnish: word-level 20% OOV → **0%** at morph level, WER 56%→32% `[A, report 04 §1/§3, Hirsimäki et al. 2006]` | Rung 1 (decomposition **+ full `syn_fs`**) is 93.5-100% singleton in all four grammars `[M, report 13]` — but that number is contaminated by the feature bundle; bare morpheme-sequence density is not separately measured | Likely workable — this is D4's intra-word term's own premise, already relied on | Workable | Dense |
| **POS + full `syn_fs`** (rung 2) | the whole feature bundle | Class count measured directly: Sena **47**, Amharic **38**, Indonesian **3**, Aweti **41** classes, over corpora of 15,804 / 184 / 106 / 148 confirmed analyses respectively `[M, report 13]` | **Already dense for Sena at ~15.8k analysis-tokens**: mean class size 336, only 4.26% singleton `[M]`. Amharic at ~184 analyses is thinner (31.58% singleton) — same rung, different density, because the corpus is two orders of magnitude smaller `[M]` | Dense for Sena-shaped grammars; still developing for Amharic-shaped ones | Dense | Dense |
| **POS + feature subset** (rung 3, per-POS) | agreement-relevant features only, chosen per POS category | Not independently measured — report 13's proxy collapsed to rung 2 in all four grammars because none declares a separate top-level `foot` feature (`[M]`, report 13's caveat 8) | Expected ≥ rung 2's density (a coarsening) | Dense | Dense |
| **POS + `mpr`** (rung 4) | MPR bitset alongside POS | **Byte-identical to rung 5** in 3 of 4 grammars — Sena 24=24, Amharic 6=6, Indonesian 3=3; only Aweti differs, 18≠16 `[M, report 13]` | Dense where it exists at all, but usually adds nothing — treat as a live per-grammar gate, not a floor (report 13's finding 3) | Dense | Dense |
| **Bare POS** (rung 5) | part of speech only | Sena 24, Amharic 6, Indonesian 3, Aweti 16 classes `[M, report 13]` | Dense at any corpus size tested, including the smallest (121-208 words) `[M]` | Dense | Dense |
| **Open/closed class** (rung 6, the floor) | the coarsest split | Sena 3, Amharic 2, **Indonesian 1** (every confirmed analysis fell in one open-class POS in that corpus), Aweti 2 `[M, report 13]` | Trivially dense, but Indonesian's single class shows the floor can be *useless*, not just coarse — no report-13 field backs this rung at all (it is a post-hoc heuristic, "the least trustworthy number in this report" per report 13's own caveat 9) | Dense but weak | Dense but weak | Dense but weak |

**Reading the table for an implementer.** The two rungs report 13 already killed (rung 1
universally, rung 4 on 3/4 grammars) stay dead here — nothing in this analysis revives them. The
useful new information is that **lemma sits at a different point on the table than any existing
rung**: its cardinality (order 10²-10³ in these small reference grammars, and stated as a
10⁴-10⁵-entry target for full-scale grammars per the standing "build for full-scale grammars"
policy) is far below wordform-type cardinality but far above rung 2's class counts (47 at most).
That ordering — 47 (rung 2) ≪ ~1,400 (lemma) ≪ ~7,000 (observed wordform types) ≪ 10⁴-10⁸ (the full
inflected-form space per stem) — is itself the measured shape of "a genuine sweet spot," which is
exactly what §2 argues from.

---

## 2. Is a stem/lemma n-gram a distinct and worthwhile rung?

**Verdict: yes, add it — but as a fourth term alongside D4's two, not as a rung inside D4's
existing ladder.** The ladder coarsens one variable (grammatical class); lemma is a second,
orthogonal variable (lexical identity) that the ladder actively discards at every rung. "Drink" and
"eat" are both, say, `V` with identical `syn_fs` in many contexts — POS+feature conditioning cannot
distinguish "drink water" from "eat water" because it never looks at which verb it is. A lemma
bigram is the only term in this design that can.

### The founding literature already names stem as a factor — this is not a novel proposal

Report 04 quotes Bilmes & Kirchhoff's own formalism directly: an FLM token is "a vector of parallel
factors, `w_i = {f_i^1, ..., f_i^k}` (**e.g., word, stem, morphological class, POS**)"
`[A, report 04 §2, Bilmes & Kirchhoff, ACL N03-2002]`. Stem is one of the four canonical example
factors in the paper that invented factored LMs — named in the same breath as POS and morphological
class, the two factors D4's ladder already uses. **D4's ladder implements two of the founding
paper's four canonical factors and silently drops the third** (word/surface is the fourth, already
rejected by D1 for the reasons report 04 §1 gives). This is the strongest single piece of evidence
that a lemma factor is not an add-on but a completion of an already-adopted formalism.

Report 04 also independently gestures at this in its own recommendation, in passing and without
separately evaluating it: "an ordinary interpolation of two or three n-gram tables (**word, POS-tag,
lemma**)" (report 04 §3). So the idea has been sitting, unexamined, in the series' own prior art
since report 04 — this report is the first to make it a decision point.

### Is there a measured lemma-vs-POS-vs-surface comparison at small data? Honest answer: no

Searching the fetched evidence across reports 04, 06, 09, 13 for a controlled comparison
("lemma-level n-gram beats POS-level and surface-level, same task, same corpus, varying only the
conditioning unit") **turns up nothing**. The closest adjacent results:

- **Tachbelie, Abate & Menzel (Amharic, HLT 2011)** — morpheme-based LMs gave "a slight
  improvement," factored LMs gave "notable improvement," over a word-bigram baseline via lattice
  rescoring `[A, report 04 §2]`. The paper's factored LM almost certainly includes a stem-like
  factor (Amharic FLM work in this tradition typically does), but the excerpt available does not
  decompose the gain by factor, so this cannot be read as "lemma specifically beat POS
  specifically" — only as "some factored combination beat plain words."
- **Vergyri, Kirchhoff, Duh & Stolcke (Arabic, Interspeech 2004)** — reports perplexity/WER
  reductions from morphology-based factored LMs, magnitude unextracted (PDF failure, report 04 §2).
  Same limitation: direction confirmed, decomposition by factor not available.
- **Class-based interpolation (Brown et al. 1992 / RNNLM clustering lit)** — the 3%-vs-19%
  perplexity-reduction split `[A, report 04 §3]` is about POS/frequency-cluster classes, not lemma;
  it establishes that *interpolation with a class term helps*, not that lemma is the right class.

**This is this report's own gap, stated plainly rather than argued around**: the case for a lemma
rung rests on (a) the founding formalism naming stem as a canonical factor, and (b) the measured
cardinality argument below — not on a measured lemma-specific accuracy gain, because no such
measurement exists in the literature this series has been able to reach. Treat "lemma bigrams help"
as architecturally well-motivated and empirically untested, the same epistemic status report 09
gives the reranker-needs-less-data hypothesis (report 09 §3, "the field's own design choices
corroborate it; no controlled experiment measures it").

### The cardinality argument — the genuine sweet spot, measured

This is where the lead's intuition gets real numbers behind it. §1's table already lays it out:
lemma cardinality sits strictly between the class-LM rungs (Sena's rung 2 has 47 classes) and the
observed-wordform-type inventory (Sena has 6,973), which itself sits far below the true inflected
space (10⁴-10⁸ forms per stem, the number the entire architecture exists to avoid enumerating). The
~~measured lemma:wordform-type ratios — Sena 1:4.8, Amharic 1:5.2, Indonesian 1:1.8 `[M, report 13]`~~

> **Corrected on review, 2026-07-25 (parent session).** Those ratios were mis-tagged and should not
> be quoted. Their numerator is report 13's **`LexEntry` count for the grammar's lexicon** (Sena
> 1,462 — not 1,464; Amharic 130; Indonesian 66, at `13-first-measurements.md:181,205-206`) and
> their denominator is the size of a separately-collected **wordform type list** (6,973 / 673 / 121).
> The two come from different populations: the wordform list is not generated from the lexicon, the
> lexicon contains entries the list never attests, and at Sena's 49.20% coverage more than half of
> those 6,973 forms confirm no analysis and therefore have no resolvable lemma at all. The count of
> *distinct lemmas attested in running text* — the quantity the argument actually needs — is unknown
> and is bounded above by, not equal to, the `LexEntry` count. Retag as `[S]`; the ratio is an
> upper-bound sketch, not a measurement.

The structural claim the ratios were offered in support of **stands on its own and does not need
them**: lemma count is bounded by the *lexicon* (a fixed, authored artifact) while wordform-type
count grows with the corpus (an open-ended, Zipfian-tailed distribution), so lemma inventory
saturates and wordform inventory does not. That is the reason a lemma bigram should be estimable at
data volumes where a surface bigram is hopeless, without the class LM's total discard of lexical
identity. What is missing is a real measurement of attested-lemma cardinality, which requires
running text — see report 18. **Added to §3's experiment list.**

### Where in the ordering

Not in D4's existing ladder — it conditions on a different variable. Two placements were
considered:

1. **Inside D4's ladder**, e.g. "lemma+POS" inserted between the dead rung 1 and rung 2. **Rejected**:
   this conflates two axes into one combinatorial rung, which reproduces exactly the sparsity rung 1
   already died of (rung 1 is decomposition+full-`syn_fs`, already 93.5-100% singleton `[M]`;
   crossing lemma with POS+`syn_fs` before backing either off separately would very likely land in
   the same dead zone, though this specific combination was not measured and should be checked
   before assuming it, not asserted here).
2. **As an independent fourth additive term**, structurally identical to how D4 already composes the
   error-model cost, the inter-word class trigram, and the intra-word morpheme n-gram. **Recommended.**
   This is the generalized-parallel-backoff (GPB) shape report 04 describes — "no obvious natural
   (temporal) backoff order... multiple dynamic backoff strategies" `[A, report 04 §2, Bilmes &
   Kirchhoff]` — applied at the coarsest possible grain: two independent factor streams (grammatical
   class, lexical identity), each with its *own* small backoff (lemma bigram → lemma unigram → skip
   when the root is unseen or the observed count is a singleton), rather than one searched
   cross-product graph. This sidesteps the exact cost D4 already flagged as too expensive to build
   first ("Start with a fixed, hand-chosen backoff graph — the searched-graph version is the
   doubling, and is not needed to ship," `PLAN.md` § D4) — a lemma bigram with a two-step internal
   backoff is a second *fixed* graph, not a search over graphs.

Composition becomes:

```
score(candidate) = w_err   * error_cost
                 + w_inter * log P(class | context)
                 + w_intra * log P(morphemes | class)
                 + w_lemma * log P(lemma | lemma_context)
```

`w_lemma` defaults low, tuned by the same low-dimensional grid search as the other three (§4) — not
a new tuning methodology, one more scalar in an existing one.

---

## 3. Corpus size selects the rung — made concrete

D15 states corpus size selects the rung as a principle; this section attaches numbers where numbers
exist and names the experiment where they don't. **No report in this series ever swept corpus size
for a fixed grammar** — report 13 measured each of four grammars once, at whatever corpus it had.
So most of this table is order-of-magnitude reasoning from a single measured point, not a curve.
State that plainly rather than implying more precision than exists.

| Rung / unit | Measured density point | Order-of-magnitude threshold | Confidence |
|---|---|---|---|
| Surface wordform | Not applicable — already rejected (D1) | Likely still sparse even at 10⁶ tokens for morphologically rich languages, per Turkish/Finnish extrapolation `[S, from report 04 §1]` | Low — no PanGloss-scale measurement exists; report 04 itself calls this "a genuine gap... nobody publishes trigram-miss rates for 50k-token corpora" |
| **Lemma bigram** | Not measured directly; cardinality (§2) known | **Unknown** — plausibly comparable to English word-bigram behaviour (workable in the 10⁴-10⁵ range) by the lexicon-bounded-cardinality argument, but this is a scaling guess, not a curve `[S]` | Low |
| Morpheme sequence (bare) | Finnish: 0% OOV at morph level `[A, report04]`, but that is a different language/corpus, not a PanGloss grammar | Workable well below 10⁴ tokens, by analogy | Medium — real analogy, wrong corpus |
| **Rung 1** (decomp+`syn_fs`) | 93.5-100% singleton at Sena's ~15.8k analyses `[M]` — the one rung with an actual negative data point | Still dead at ~1.5×10⁴ analysis-tokens; true threshold, if any, is unknown and could be far above 10⁶ | Medium (we know where it fails, not where it would succeed) |
| **Rung 2** (POS+`syn_fs`) | **Already dense for Sena** at ~15.8k analyses (mean class size 336, 4.26% singleton) `[M]`; **still thin for Amharic** at ~184 analyses (31.58% singleton) `[M]` | Sena-shaped grammars: dense by ~10⁴ tokens. Amharic-shaped grammars (38 classes, thinner corpus): threshold not yet reached at ~2×10² tokens; unknown how much more is needed | High for the Sena data point; the Amharic contrast is itself the proof that the threshold is **per-grammar**, exactly as D15 states, not a universal number |
| Rung 3 (feature subset) | Not independently measured (proxy collapsed to rung 2 in all four grammars, report 13 caveat 8) | Expected ≥ rung 2's threshold (coarser) | Low |
| Rung 4/5 (`mpr`/POS) | Dense at every measured corpus, 121-6,973 words `[M]` | No threshold problem — dense at the smallest sizes tested | High |
| Rung 6 (open/closed) | Dense at every measured corpus, but Indonesian's single class shows density ≠ usefulness `[M]` | No threshold problem, but treat as a last resort, not a target | High for density, low for value |
| **Warm cache frequency ranking** (D14) | D15 already states this needs "the most" text and is "poor" at small size — a stated, not measured, ordering | Unknown; D15 explicitly names this "the top unknown" ahead of any ranking question | None — flagged by the plan itself as unresolved |

### The experiment to run, rather than the number to guess

The corpus-size sweep report 13 did not do is buildable now, at zero marginal engine cost, because
the harness already exists: `rust/crates/pg-cli/examples/spellcheck_measure.rs` (report 13's
dev-only harness) already computes rung cardinality/singleton-rate per grammar from a wordform list.
Extend it to run at multiple **synthetic** corpus sizes (10³, 10⁴, 10⁵, 10⁶ tokens), generated by
sampling the grammar itself — the same generative capability D14's warm-cache build already needs —
at two skew settings (uniform-over-paradigm-cells vs. root-frequency-weighted/Zipfian), and plot
singleton-rate and mean-class-size against corpus size, per rung, per grammar. This directly reuses
existing tooling rather than inventing new. **Two things this sweep alone will not answer, and
should be scoped separately**: (1) cardinality/density is a *necessary* condition for estimability,
not a *sufficient* one for predictive value — a follow-up pass should hold out a slice and measure
actual perplexity/precision@1 at each size, not just class density; (2) synthetic corpora inherit
whatever skew the sampler assumes, so the sweep's numbers are only as realistic as the sampling
distribution, which is itself an open, unvalidated choice (paralleling D15's own flagged risk that a
Scripture-trained corpus has different skew than phone-typed text).

---

## 4. Combining several models

D4's composition — `w_err*error_cost + w_inter*logP(class|ctx) + w_intra*logP(morph|class)`, plus
§2's proposed `w_lemma` term — is **already a log-linear combination**, not literally the
probability-weighted "linear interpolation" of the classic Jelinek-Mercer sense. This distinction
matters for the rest of this section, so it is worth being precise about it before comparing
schemes.

| Scheme | Mechanism | Data needed to fit | Where it sits relative to D4 |
|---|---|---|---|
| **Linear interpolation** (Jelinek-Mercer) | `P = Σ λᵢPᵢ(w)`, `λ`s sum to 1, fit by EM/grid search on held-out data, often bucketed by the finer model's observed count | Low for a single global `λ`; grows per bucket if `λ` is context-conditioned | Report 04's word⊕class-LM interpolation recommendation (§3, the 3%/19% split) is exactly this scheme, at the probability level |
| **Log-linear / MaxEnt** | `score = Σ wᵢfᵢ(x)`, weights unconstrained, normalized only if a probability is needed | **Depends entirely on the number of features.** A handful of scalar terms (D4's actual shape) is as cheap to fit as linear interpolation's `λ`s. A real MaxEnt/CRF with many templated features (report 08's LEMMING) needs ~100K tokens | **D4 as specified is this scheme, in its cheap 3-4-scalar special case** — not the expensive many-feature case the "MaxEnt is data-hungry" folk wisdom usually warns about |
| **Backoff cascade** | Hard fallback: use the finer model if data-sufficient, else fall through; no blending, winner-take-all | None to fit beyond a sufficiency threshold | This is what D4's rung ladder and D9's tiers already are — a *different* problem (choosing among rungs of the *same* factor) from this section's question (combining *separate* models: class n-gram, morpheme n-gram, lemma bigram, cache) |
| **Mixture-of-experts** | A learned gate decides, per context, which expert(s) to trust and by how much | High — now fitting a *function* of context, not a handful of global scalars; more data-hungry than either interpolation or a small log-linear combination by construction | **The scheme to flag as needing more held-out data than PanGloss will ever have** — see below |

### Is D4's grid search on ~760 tokens defensible? Yes, for the shape it actually is

Report 09 §7 independently arrives at the same recommendation D4 already made, and gives it a
concrete justification: "Grid search over λ on whatever small hand-annotated gold set exists, even
if it's only 50-200 sentences — a single scalar... is a low-dimensional search, and low-dimensional
hyperparameter search is exactly the regime where tiny gold sets remain informative even when
they're far too small to train or fully evaluate a model on" `[A, report 09 §7]`. Report 09's own
cited comparables for "workable annotated set size" — Malagasy 383 sentences/5,294 tokens,
Kinyarwanda 196 sentences/4,882 tokens `[A, report 09 §6]` — are both *smaller* than Sena 3's 760
gold analyses (`PLAN.md` § D13's measured table). By report 09's own standard, D4's gold set is
comfortably, not marginally, inside the workable range for a 3-4 dimensional grid search.

D4 also already does report 09's option 3 simultaneously with option 1: "defaulting conservatively
toward the error-model term" (`PLAN.md` § D4) is exactly report 09's "conservative-by-construction
defaults... require the reranker to earn weight via measured held-out gains" `[A, report 09 §7]`.
Grid search plus a conservative floor is the most defensible combination report 09 lays out, not a
naive single-shot choice — worth stating explicitly, since it is easy to read "grid search on 760
tokens" alone and worry, without noticing the plan already pairs it with the mitigation the
literature recommends.

### Alternatives, and what they cost

- **Leave-one-out / k-fold over the gold set.** A refinement, not a replacement: run the same
  low-dimensional grid but score it via LOO/k-fold over the 760 tokens instead of one static split.
  Squeezes more signal from the same data at near-zero extra cost for a 3-4 scalar grid.
  Recommended as a strict improvement to what D4 already specifies.
- **Held-out interpolation à la Jelinek-Mercer, bucketed by context count.** This is more
  data-hungry than D4's single (or few) global weight(s) specifically *because* it wants a separate
  `λ` per count-bucket — each bucket needs its own held-out mass. A single global weight (D4's
  actual shape) is fine at 760 tokens; bucketing into more than 2-3 count-strata starts running
  into the same problem MoE has, just less severely. **Recommendation: do not bucket the weights
  without measuring first that a single global value underperforms.**
- **Bayesian priors toward the coarser model.** A formal treatment — e.g. a prior on `w_lemma`/
  `w_inter`/`w_intra` with its mode at "the coarser/simpler model dominates," updated by the 760
  gold tokens as a small number of pseudo-observations — would likely converge near D4's already-
  stated conservative default, but with a principled credible interval instead of a bare point
  estimate. The concrete value this adds: a wide credible interval on `w_lemma` (say) is itself
  useful, legitimate output ("we don't have enough signal to say this term earns real weight yet")
  where a grid search alone would just report a possibly-noisy optimal point without saying how
  noisy. Worth doing if the implementation cost is small; not worth blocking shipment on.

### What to flag as needing more held-out data than we will ever have

**Mixture-of-experts, unambiguously.** A learned per-context gate is a function to fit, not a
scalar to search — categorically more data-hungry than anything else in this section, by the same
logic report 08 gives for why a real CRF reranker needs ~100K tokens (report 08, cited in `PLAN.md`
§ D5) and by the same overfitting-at-tiny-N pattern Gboard's own engineers had to explicitly
counter with key-clustering when personalizing spatial models per-user `[A, report 06 §2, Ghosh et
al. 2022]` — if Google needs anti-overfitting machinery for a per-user gate at Gboard's per-user
data volumes, a per-context gate at PanGloss's *population*-wide 760-token gold set is far past the
point of being estimable. **Also flag**: any log-linear combination that grows beyond a handful of
hand-chosen scalar terms into a templated many-feature CRF/MaxEnt model, which is exactly the
architecture report 08/D5 already scope as a *bounded late ablation*, never the shipped design.

---

## 5. Self-updating / online adaptation

### Cache language models and the recency effect

**Kuhn & De Mori 1990** introduced the mechanism directly: an n-gram estimated from only the recent
word history, linearly combined with a static trigram, on the premise that a recently-occurring
word is more likely to recur than its static frequency alone predicts `[A, report 06 §1]`.
**Goodman 2001**'s controlled ablation (a *different* Goodman paper from the smoothing study report
04 §6 cites) isolated caching from higher-order n-grams, skipping, KN smoothing, and clustering, and
found caching alone worth **up to 0.6 bits of perplexity improvement on small training data**,
reported as one of the single most powerful techniques tested in isolation `[A, report 06 §1,
Goodman 2001]`. This is the strongest available number for "does a recency signal help at tiny
personal-data scale" — and it is specifically a within-domain repetition effect (a word this
speaker/document just used is disproportionately likely to recur), which is the exact shape of a
personal-overlay update, not a claim about learning a new distribution from a handful of samples.

### Count decay and half-life — a real gap, not glossed over

**No report in this series states a specific decay half-life or count-decay constant.** Kuhn & De
Mori's own mechanism, as characterized in report 06, is a bounded-recent-history window rather than
an explicit exponential decay — the qualitative "recency helps" finding is solid; the specific
numeric schedule is not sourced anywhere this series reached. **This should be treated as unknown,
not guessed.** The concrete recommendation: whatever decay mechanism ships, its half-life is one
more scalar, tuned by the exact same low-dimensional grid-search discipline §4 already establishes
as workable at PanGloss's data scale — not a separate research problem, and not something to pick
from intuition either.

### Online adaptation of interpolation weights

Dynamic re-estimation of interpolation weights (as opposed to a fixed global value) is the standard
mechanism in the adjacent literature, typically driven by the posterior over the current
topic/dialogue state, with one cited result reporting a **13.52% relative WER reduction** from
dynamic vs. static interpolation weighting in a dialogue-ASR task `[A, report 06 §1]` — flagged
there as secondary/asserted, task and corpus not independently confirmed. This is directly relevant
to whether `w_cache` (below) should itself adapt online, as opposed to only the *counts* it scores
over adapting — see §7 for why the answer here should be "no, not the weight, only the counts,"
mirroring D9's reasoning for the unseen-form penalty.

### Per-user adaptation without catastrophic drift

**Gboard's spatial-model personalization** (Ghosh et al. 2022) is the closest deployed precedent:
learning a per-user key-center offset (and optionally covariance) on top of a shared Gaussian touch
model, explicitly engineering against tiny-per-user-N overfitting via **key-clustering to borrow
strength across related keys** `[A, report 06 §2]`. **Federated Reconstruction** (Singhal et al.,
NeurIPS 2021) names the general pattern underneath this: some parameters (per-user offsets) are
*never* aggregated to a server, ever — they live and stay local, while a shared component is what
optionally gets federated `[A, report 06 §2]`. Both precedents converge on the same discipline: a
per-user parameter must **shrink toward the shared/base prior when the personal sample is small**,
which is precisely what Kneser-Ney backoff already does for n-gram counts, applied one level up to
personalization itself. This is the anti-catastrophic-drift mechanism, and it is already the
pattern report 06 recommends for PanGloss's personal confusion-model layer (report 06's
Recommendations (a)2) — it generalizes cleanly to every personal term proposed here, including the
cache term below.

### The structural finding: D9's tier-0 cache IS a cache language model — and the plan doesn't say so yet

This is the sharpest, most actionable point in this section. Compare the two mechanisms directly:

- **Kuhn & De Mori's cache LM**: an n-gram estimated from recent word history, **linearly combined**
  with a static model, on the premise that recent occurrence predicts recurrence beyond static
  frequency `[A, report 06 §1]`.
- **D9 tier 0**: "Cache of words SEEN — typed by this user, or present in this document; persisted
  across sessions... always, emitted immediately" (`PLAN.md` § D9). **D8b**: "'Already-typed words'
  and 'common words in this session' are observable without any hook at all; we accumulate
  frequency counts from context" (`context.left`, delivered on every `predict()` call,
  `PLAN.md` § D8b).

These are the same mechanism. D9/D8b's seen-word cache is a within-session/within-document recency
signal, accumulated from exactly the "recent word history" Kuhn & De Mori's cache term is built
from, and D9's ranking rule already specifies that it must be combined with the base score
(currently via a hard tier + large fixed penalty, not a soft weight — see below for the one real
difference). **Three specific differences from the textbook cache LM, all worth naming precisely
rather than leaving implicit:**

1. **No decay, and an unbounded window.** D9/D14 persist the cache "across sessions" and ship a
   pre-computed 10k-entry warm cache alongside it (D14) — this is closer to a permanently
   accumulating unigram cache with a static prior floor than Kuhn & De Mori's bounded recent-history
   window. Adding real decay (§ above) would turn the current all-or-nothing accumulation into the
   textbook mechanism, and Goodman 2001's 0.6-bit number suggests that decayed recency, not just
   permanent presence/absence, is worth real accuracy — not just candidate-set membership.
2. **A hard tier + fixed penalty, not a soft interpolation weight.** D9 currently uses the cache as a
   *candidate-supply* gate with a large, constant, non-learned separation between seen and unseen
   (`PLAN.md` § D9, "The ranking rule") — a backoff-cascade shape, not the linear-interpolation shape
   Kuhn & De Mori's own cache term uses. This is not necessarily wrong (D9's own justification —
   latency/anytime concerns, and the same starved-data circularity §7 discusses — applies to *why*
   the seen/unseen boundary is hard-coded), but it means the *recency-weighted frequency itself*,
   once decay is added, should also feed a **soft ranking term within a tier**, exactly as D9's own
   text already permits: "Hard-code the ordering and let D4's terms rank *within* a tier"
   (`PLAN.md` § D9). This is not a proposal to replace D9's tiering; it is a proposal to let the
   cache be scored, not merely gated, once inside its tier — a `w_cache * log P_cache(w)` term
   alongside D4's other three (soon four).
3. **Does the plan currently treat it as a cache LM? No — checked directly.** `PLAN.md` never uses
   the term "cache language model," never cites Kuhn & De Mori, and frames D9/D14/D8b entirely as a
   *supply* mechanism ("Tiers govern supply, never flagging" — D9) rather than a *probability term*.
   Recognizing the equivalence costs nothing to implement (the accumulation mechanism is already
   built) and buys two things for free: (a) the ~60-year-old cache-LM literature's tuning wisdom
   (decay, interpolation-weight estimation) becomes directly applicable prior art rather than
   something to re-derive; (b) it clarifies that **part of "self-updating" is already decided**,
   not an open research question — the open part is narrowly the decay schedule and the soft-scoring
   integration, not the existence of the mechanism.

---

## 6. Cold start and the adaptation curve

**What the user sees on day 1, stated plainly.** D14's shipped warm cache means day 1 is never a
blank slate: the full D4 composition (error model + inter-word class n-gram + intra-word morpheme
n-gram, plus §2's lemma term if adopted) runs over a pre-ranked ~10k-entry cache from install. There
is no "dumb autocomplete" period — the generic, non-personalized speller is fully functional from
the first keystroke. What is missing on day 1 is **only** the personal layer: the user's own
seen-word delta (D8b, IndexedDB) starts empty and must accumulate before it can outrank a
shipped-but-wrong or shipped-but-lower-frequency competitor.

**Measured adaptation curves from the gathered literature: none directly transferable.** The
closest quantitative figure is Gboard's federated OOV-discovery convergence, "≈2000 rounds over ≈4
days" `[A, report 06 §4, Chen et al. 2019]` — but that is a **global model retraining** number at
Google's population scale (server-side FL rounds across many devices), not a single device's local
adaptation speed, and should not be read as an answer to "how many keystrokes before *this* user's
cache measurably helps." No report in this series contains a per-device, per-keystroke cold-start
curve for predictive text or IME adaptation. **This is flagged as unknown, not estimated by
analogy** — the mechanisms are too different (server-aggregated retraining vs. local accumulator)
for the number to transfer even approximately.

**Why the remaining question is smaller than it looks.** Because D14 already moved the expensive
part (generation) to build time, and because D9's own open item already asks "whether rungs 1 and 2
[user-typed vs. shipped-warm-cache] need a smaller separation... a user's own history should
outrank a shipped prior" (`PLAN.md` § D14), the practical cold-start question reduces to a single
accumulation-threshold parameter — how many times must a user type/accept a personal form before it
crosses that (currently unspecified) smaller separation — not a whole-model retraining question.
This is exactly the shape of thing report 10/D10's standing policy already covers: "measure, don't
guess," using the same per-grammar calibration harness pattern already adopted for tier latency
(`PLAN.md` § D10). **The experiment to run**: instrument the shipped D8b accumulator and log, either
via synthetic simulated typing sessions or opt-in field telemetry, how many keystrokes/session
elapse before a repeated personal form's combined score (cache term + base composition) overtakes a
shipped competitor at the same or adjacent tier — a small, bounded measurement, not a research
project, and one that reuses D10's already-adopted "measure per-grammar, don't infer from a formula"
discipline rather than inventing a new one.

---

## 7. What must never be adaptive

D10 already forbids adapting recall: tier policy may trade latency for candidate-set size, never
for a correctness guarantee, and a budget shortfall must be a *stated* degraded mode, never a silent
one (`PLAN.md` § D10). The reasoning generalizes to a short, principled list of other quantities that
must stay fixed, all sharing the same underlying logic — **a system this data-starved cannot be
allowed to estimate its own correction factor from the same starved data it is meant to correct
for**:

| Must stay fixed | Why | Precedent in the plan |
|---|---|---|
| **D9's unseen-form penalty** | Named explicitly already: "a constant, not a learned weight... at 50k tokens *everything* is rare, so a learned penalty would be estimated from the same starved data it is supposed to correct for" | `PLAN.md` § D9 |
| **The tier ordering / cascade shape itself** (seen > shipped-cache > generated) | Same starved-data circularity as above — only ranking *within* a tier is D4's job; the order of tiers is not something a device should renegotiate from its own limited traffic | `PLAN.md` § D9/D14 |
| **D12/D13's admission gates** (orthography well-definedness, corpus-recall certification) | Per-language yes/no scope decisions made at build/ship time by people who can see the whole picture; a device silently deciding a language now qualifies is a correctness/scope change, not a ranking one | `PLAN.md` § D12, D13 |
| **Coverage** (the fraction of tokens that get ≥1 confirmed analysis) | Same class as recall — trading it for speed or cache convenience must be a stated degraded mode, never silent, per D10's rule extended to this axis | `PLAN.md` § D10, D15 |
| **The class-defining inventories** (POS set, `syn_fs` feature space, morpheme inventory, and — after §2 — the `LexEntryId` space) | D15's binding mechanism keys the add-on to a content digest over exactly these; a personalized on-device model must not silently outlive a grammar update that changes what these inventories mean | `PLAN.md` § D15 |
| **The shipped weights of the log-linear composition** (`w_err`, `w_inter`, `w_intra`, `w_lemma`, `w_cache`) — new item, this report | If per-user online reweighting is ever added, it should shrink toward, never freely drift from, the offline-calibrated defaults — the same anti-overfitting shrinkage Gboard's per-user spatial model already needed `[A, report 06 §2]`, applied to these weights rather than to key offsets. Only the *counts* the cache term scores over should update freely; the *weight* attached to that term should not | New, but directly continuous with D9's unseen-penalty reasoning and report 06's shrinkage-to-prior pattern |

---

## 8. Verdict — the concrete shipping set

**Four statistical terms ship simultaneously, composed in one unified weighted log-linear scoring
function — not four separate systems, not a cascade of independently-invoked models, not
mixture-of-experts:**

```
score(candidate) = w_err   * error_cost
                 + w_inter * log P(class | context)          [D4, decided — 4-rung ladder, report 13's dead rungs already dropped]
                 + w_intra * log P(morphemes | class)        [D4, decided]
                 + w_lemma * log P(lemma | lemma_context)    [new, §2 — own tiny backoff: bigram → unigram → skip]
                 + w_cache * log P_cache(w)                  [recognizing D9 tier-0 + D8b as a cache LM, §5]
```

**Which parts are static (shipped, versioned, offline-built) vs. adaptive (on-device, per-user):**

| Component | Ships as | Updates on-device? | Rate |
|---|---|---|---|
| Inter-word class n-gram, intra-word morpheme n-gram, lemma bigram | Static, part of the D15 corpus-trained add-on, versioned separately from `.pgpack` per D15's binding rule | No — retraining needs a real corpus and a design review, not a phone | Rebuilt offline when the corpus or the class-defining inventories change (D15) |
| Warm cache (D14) frequency ranking | Static, pack-build-time, ~10k entries | No | Rebuilt at pack-build time |
| **Cache term `P_cache(w)`** (D9 tier-0 counts + D8b accumulation) | Ships empty (or seeded from the warm cache, per D14) | **Yes — this is the one genuinely adaptive statistical component** | Continuous: every keystroke via `context.left`, no explicit hook needed (D8b); recommended addition of real decay, half-life tuned offline (§5), not guessed |
| All five weights (`w_err`…`w_cache`) | Static, grid-searched per grammar on the ~760-token gold set (§4) | **No** — per §7's new item, these must not freely drift on-device even if per-user reweighting is ever explored later | Recalibrated offline, same discipline as D10's tier calibration |

**What does not ship, and why:** no mixture-of-experts gate (§4 — needs a function fit, not a
scalar, and nothing supports that being estimable at this data scale); no per-user retraining of the
corpus-trained add-on itself (that stays D15's versioned, offline artifact); no on-device
reweighting of the log-linear composition's weights (§7); no cross-user aggregation for small
communities (D7, reaffirmed directly by report 06 §9's math — every surveyed deployed system
operates 3+ orders of magnitude above a minority-language speaker population, and PanGloss's
smallest target communities sit in the hundreds).

**What is genuinely new relative to the plan as it stands, in priority order for whoever picks this
up:** (1) recognize D9 tier-0 as a cache LM explicitly, and add real decay to it (§5) — cheapest,
highest-value change, touches no architecture, only adds a scalar and a scoring path; (2) add the
lemma bigram as a fourth term with its own two-step backoff (§2) — a small, bounded addition to an
already-adopted formalism, not a new one; (3) run the corpus-size sweep on the existing
`spellcheck_measure.rs` harness (§3) to replace this report's order-of-magnitude reasoning with an
actual curve; (4) instrument the cold-start accumulation threshold once a beta channel exists (§6)
— the one item on this list that cannot be answered from a repo-only experiment.
